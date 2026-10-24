//! Native serial, UDP and bounded Rust ZMQ PUB FicTrac row outputs.
use crate::{ConfigDocument, Error, Result, write_dat};
use fictrac_core::TrackRow;
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::Duration,
};

pub struct OutputSinks {
    udp: Option<(UdpSocket, SocketAddr)>,
    serial: Option<serial2::SerialPort>,
    zmq: Option<crate::zmq_output::ZmqOutput>,
}
impl OutputSinks {
    pub fn disabled() -> Self {
        Self {
            udp: None,
            serial: None,
            zmq: None,
        }
    }
    /// Reuse healthy outputs when their settings are unchanged. Otherwise close
    /// exclusive handles before reopening them, restoring the previous settings
    /// if the replacement fails. A failed rollback leaves outputs disabled and
    /// is explicitly reported; numerical/session state belongs to the caller.
    pub fn reconfigure(&mut self, old: &ConfigDocument, new: &ConfigDocument) -> Result<()> {
        let unchanged = [
            "sock_host",
            "sock_port",
            "com_port",
            "com_baud",
            "zmq_host",
            "zmq_port",
            "zmq_queue_capacity",
        ]
        .iter()
        .all(|key| old.get(key) == new.get(key));
        let healthy = self.zmq.as_ref().is_none_or(|output| {
            let status = output.status();
            status.worker_alive && status.failures == 0
        });
        if unchanged && healthy {
            return Ok(());
        }
        let previous = std::mem::replace(self, Self::disabled());
        drop(previous);
        match Self::from_config(new) {
            Ok(outputs) => {
                *self = outputs;
                Ok(())
            }
            Err(error) => match Self::from_config(old) {
                Ok(outputs) => {
                    *self = outputs;
                    Err(Error::Config(format!(
                        "output configuration failed: {error}; rollback succeeded; previous output settings restored"
                    )))
                }
                Err(rollback) => Err(Error::Config(format!(
                    "output configuration failed: {error}; rollback failed: {rollback}; outputs are disabled; reapply a valid output configuration"
                ))),
            },
        }
    }
    /// Use an already configured native serial port (also useful for PTY harnesses).
    pub fn from_serial_port(port: serial2::SerialPort) -> Self {
        Self {
            udp: None,
            serial: Some(port),
            zmq: None,
        }
    }
    /// Open explicitly configured outputs; disabled ports are never contacted.
    pub fn from_config(document: &ConfigDocument) -> Result<Self> {
        let zmq_settings = crate::zmq_output::ZmqSettings::parse(
            document.get("zmq_port"),
            document.get("zmq_host"),
            document.get("zmq_queue_capacity"),
        )?;
        let port = document
            .get("sock_port")
            .map(str::parse::<i32>)
            .transpose()
            .map_err(|_| Error::Config("sock_port must be an integer from -1 to 65535".into()))?
            .unwrap_or(-1);
        if !(-1..=65535).contains(&port) {
            return Err(Error::Config(
                "sock_port must be -1 (disabled), 0 (disabled), or 1..65535".into(),
            ));
        }
        let udp = if port > 0 {
            let host = document.get("sock_host").unwrap_or("127.0.0.1");
            let destination = (host, port as u16)
                .to_socket_addrs()?
                .next()
                .ok_or_else(|| Error::Config(format!("sock_host {host} has no usable address")))?;
            let bind = if destination.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            };
            let socket = UdpSocket::bind(bind)?;
            socket.set_nonblocking(true)?;
            Some((socket, destination))
        } else {
            None
        };
        let serial = if let Some(port) = document.get("com_port").filter(|s| !s.is_empty()) {
            let baud = document
                .get("com_baud")
                .map(str::parse::<u32>)
                .transpose()
                .map_err(|_| Error::Config("com_baud must be a positive integer".into()))?
                .unwrap_or(115200);
            if baud == 0 || baud > 4_000_000 {
                return Err(Error::Config("com_baud must be in 1..4000000".into()));
            }
            let mut device = serial2::SerialPort::open(port, |mut settings: serial2::Settings| {
                settings.set_raw();
                settings.set_baud_rate(baud)?;
                settings.set_char_size(serial2::CharSize::Bits8);
                settings.set_parity(serial2::Parity::None);
                settings.set_stop_bits(serial2::StopBits::One);
                settings.set_flow_control(serial2::FlowControl::None);
                Ok(settings)
            })
            .map_err(|e| {
                Error::Config(format!(
                    "cannot open serial port {port} at {baud} baud: {e}"
                ))
            })?;
            device.set_write_timeout(Duration::from_millis(100))?;
            Some(device)
        } else {
            None
        };
        let zmq = zmq_settings
            .map(crate::zmq_output::ZmqOutput::start)
            .transpose()?;
        Ok(Self { udp, serial, zmq })
    }
    /// Rejected frames are omitted, matching upstream logData.
    /// One complete newline-terminated `FT, ` record, identical for serial and UDP.
    /// Sink failures never turn missing measurements into measured zero motion.
    pub fn write(&mut self, row: &TrackRow) -> Vec<String> {
        if !row.accepted {
            return Vec::new();
        }
        let mut packet = b"FT, ".to_vec();
        if let Err(error) = write_dat(&mut packet, row) {
            return vec![error.to_string()];
        }
        let mut errors = Vec::new();
        if let Some((socket, address)) = &self.udp {
            match socket.send_to(&packet, address) {
                Ok(n) if n == packet.len() => {}
                Ok(_) => errors.push("UDP output truncated its record".into()),
                Err(error) => errors.push(format!("UDP output to {address}: {error}")),
            }
        }
        if let Some(serial) = &mut self.serial
            && let Err(error) = serial.write_all(&packet)
        {
            errors.push(format!("serial output: {error}"));
        }
        if let Some(zmq) = &mut self.zmq {
            errors.extend(zmq.enqueue(packet));
        }
        errors
    }
    pub fn status(&self) -> OutputStatus {
        OutputStatus {
            udp_enabled: self.udp.is_some(),
            udp_destination: self.udp.as_ref().map(|(_, address)| address.to_string()),
            serial_enabled: self.serial.is_some(),
            zmq: self.zmq.as_ref().map(|z| z.status()),
        }
    }
    pub fn active(&self) -> bool {
        self.udp.is_some() || self.serial.is_some() || self.zmq.is_some()
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct OutputStatus {
    pub udp_enabled: bool,
    pub udp_destination: Option<String>,
    pub serial_enabled: bool,
    pub zmq: Option<crate::zmq_output::ZmqOutputStatus>,
}
