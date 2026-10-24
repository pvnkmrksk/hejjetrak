//! Native serial and UDP FicTrac row outputs. Failures are reported per sink.
use crate::{ConfigDocument, Error, Result, write_dat};
use fictrac_core::TrackRow;
use std::{
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    time::Duration,
};

pub struct OutputSinks {
    udp: Option<(UdpSocket, SocketAddr)>,
    serial: Option<serial2::SerialPort>,
}
impl OutputSinks {
    /// Use an already configured native serial port (also useful for PTY harnesses).
    pub fn from_serial_port(port: serial2::SerialPort) -> Self {
        Self {
            udp: None,
            serial: Some(port),
        }
    }
    /// Open explicitly configured outputs; disabled ports are never contacted.
    pub fn from_config(document: &ConfigDocument) -> Result<Self> {
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
        Ok(Self { udp, serial })
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
        errors
    }
    pub fn active(&self) -> bool {
        self.udp.is_some() || self.serial.is_some()
    }
}
