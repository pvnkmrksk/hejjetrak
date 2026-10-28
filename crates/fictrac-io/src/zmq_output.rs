//! Pure Rust ZMTP PUB output, isolated from tracking by a bounded queue.
use crate::{Error, Result};
use serde::Serialize;
use std::{
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::mpsc::{self as queue, Sender};
#[path = "zmq_protocol.rs"]
mod protocol;
const DEFAULT_CAPACITY: usize = 256;
const MAX_CAPACITY: usize = 4096;
const MAX_WIRE_BYTES: usize = 16_384;
const CLOSE_DRAIN: Duration = Duration::from_millis(50);
pub const DEFAULT_ZMQ_HOST: &str = "0.0.0.0";
pub const DEFAULT_ZMQ_PORT: &str = "9871";

#[derive(Clone, Debug)]
pub(crate) struct ZmqSettings {
    pub endpoint: String,
    pub capacity: usize,
}
impl ZmqSettings {
    pub fn parse(
        port: Option<&str>,
        host: Option<&str>,
        capacity: Option<&str>,
    ) -> Result<Option<Self>> {
        let port = port
            .unwrap_or(DEFAULT_ZMQ_PORT)
            .parse::<i32>()
            .map_err(|_| {
                Error::Config("zmq_port must be an integer: -1/0 disables, 1..65535 enables".into())
            })?;
        if !(-1..=65535).contains(&port) {
            return Err(Error::Config(
                "zmq_port must be -1/0 (disabled) or 1..65535".into(),
            ));
        }
        let capacity = capacity
            .map(str::parse::<usize>)
            .transpose()
            .map_err(|_| Error::Config("zmq_queue_capacity must be an integer in 1..4096".into()))?
            .unwrap_or(DEFAULT_CAPACITY);
        if !(1..=MAX_CAPACITY).contains(&capacity) {
            return Err(Error::Config(
                "zmq_queue_capacity must be in 1..4096".into(),
            ));
        }
        let host = host.unwrap_or(DEFAULT_ZMQ_HOST);
        let address = if host == "*" {
            IpAddr::from([0, 0, 0, 0])
        } else if host.eq_ignore_ascii_case("localhost") {
            IpAddr::from([127, 0, 0, 1])
        } else {
            host.parse::<IpAddr>().map_err(|_| {
                Error::Config(
                    "zmq_host must be localhost, a numeric local IPv4/IPv6 address, or * for all IPv4 interfaces; this binds the tracking computer, not the VR receiver, and DNS names are not resolved".into(),
                )
            })?
        };
        if port <= 0 {
            return Ok(None);
        }
        let endpoint = if address.is_ipv4() {
            format!("tcp://{address}:{port}")
        } else {
            format!("tcp://[{address}]:{port}")
        };
        Ok(Some(Self { endpoint, capacity }))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ZmqOutputStatus {
    pub endpoint: String,
    pub queue_capacity: usize,
    pub queue_len: usize,
    pub enqueued: u64,
    /// Completed PUB send attempts, not subscriber acknowledgments.
    pub published: u64,
    /// Application queue overflow, failed sends or bounded-shutdown discards.
    pub dropped: u64,
    pub failures: u64,
    pub worker_alive: bool,
    pub last_error: Option<String>,
}
#[derive(Default)]
struct Counters {
    enqueued: AtomicU64,
    published: AtomicU64,
    dropped: AtomicU64,
    failures: AtomicU64,
    alive: AtomicBool,
    closing: AtomicBool,
    last_error: Mutex<Option<String>>,
}
impl Counters {
    fn fail(&self, message: String) {
        self.failures.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut error) = self.last_error.lock() {
            *error = Some(message);
        }
    }
}
pub(crate) struct ZmqOutput {
    settings: ZmqSettings,
    sender: Option<Sender<Vec<Vec<u8>>>>,
    counters: Arc<Counters>,
    worker: Option<JoinHandle<()>>,
    reported_failures: u64,
}
impl ZmqOutput {
    pub fn start(settings: ZmqSettings) -> Result<Self> {
        let (sender, receiver) = queue::channel(settings.capacity);
        let counters = Arc::new(Counters::default());
        let worker_counters = counters.clone();
        let endpoint = settings.endpoint.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let worker=thread::Builder::new().name("hejjetrak-zmq-pub".into()).spawn(move||{
            let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
                let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build(){Ok(runtime)=>runtime,Err(error)=>{let message=format!("ZMQ runtime could not start: {error}");worker_counters.fail(message.clone());let _=ready_tx.send(Err(message));return;}};
                runtime.block_on(protocol::publish_loop(endpoint,receiver,worker_counters.clone(),ready_tx));
                runtime.shutdown_timeout(Duration::from_millis(50));
            }));
            if outcome.is_err(){worker_counters.fail("ZMQ publisher stopped after an internal transport failure; reapply its configuration to restart".into());}
            worker_counters.alive.store(false,Ordering::Release);
        })?;
        match ready_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(Self {
                settings,
                sender: Some(sender),
                counters,
                worker: Some(worker),
                reported_failures: 0,
            }),
            Ok(Err(message)) => {
                drop(sender);
                let _ = worker.join();
                Err(Error::Config(message))
            }
            Err(error) => {
                drop(sender);
                counters.closing.store(true, Ordering::Release);
                let _ = worker.join();
                Err(Error::Config(format!(
                    "ZMQ publisher did not become ready: {error}"
                )))
            }
        }
    }
    /// No socket I/O, waits or locks occur on the successful tracking path.
    pub fn enqueue(&mut self, packet: Vec<Vec<u8>>) -> Vec<String> {
        let mut errors = self.take_errors();
        if packet.is_empty()
            || packet.len() > 2
            || packet
                .iter()
                .try_fold(0usize, |total, frame| total.checked_add(frame.len()))
                .is_none_or(|length| length > MAX_WIRE_BYTES)
        {
            self.counters.dropped.fetch_add(1, Ordering::Relaxed);
            errors
                .push("ZMQ output requires one or two frames within its 16 KiB wire limit".into());
            return errors;
        }
        let Some(sender) = &self.sender else {
            return errors;
        };
        match sender.try_send(packet) {
            Ok(()) => {
                self.counters.enqueued.fetch_add(1, Ordering::Relaxed);
            }
            Err(queue::error::TrySendError::Full(_)) => {
                let count = self.counters.dropped.fetch_add(1, Ordering::Relaxed) + 1;
                errors.push(format!("ZMQ output queue is full; live row dropped (total {count}); numeric files retain the measurement"));
            }
            Err(queue::error::TrySendError::Closed(_)) => {
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                if errors.is_empty() {
                    errors.push(
                        "ZMQ publisher is unavailable; reapply its configuration to restart output"
                            .into(),
                    );
                }
            }
        }
        errors
    }
    fn take_errors(&mut self) -> Vec<String> {
        let failures = self.counters.failures.load(Ordering::Acquire);
        if failures == self.reported_failures {
            return Vec::new();
        }
        self.reported_failures = failures;
        match self.counters.last_error.try_lock() {
            Ok(message) => message.iter().cloned().collect(),
            Err(_) => {
                vec!["ZMQ publisher reported an output failure; inspect output status".into()]
            }
        }
    }
    pub fn status(&self) -> ZmqOutputStatus {
        ZmqOutputStatus {
            endpoint: self.settings.endpoint.clone(),
            queue_capacity: self.settings.capacity,
            queue_len: self
                .sender
                .as_ref()
                .map(|s| self.settings.capacity.saturating_sub(s.capacity()))
                .unwrap_or(0),
            enqueued: self.counters.enqueued.load(Ordering::Relaxed),
            published: self.counters.published.load(Ordering::Relaxed),
            dropped: self.counters.dropped.load(Ordering::Relaxed),
            failures: self.counters.failures.load(Ordering::Relaxed),
            worker_alive: self.counters.alive.load(Ordering::Acquire),
            last_error: self.counters.last_error.lock().ok().and_then(|e| e.clone()),
        }
    }
}
impl Drop for ZmqOutput {
    fn drop(&mut self) {
        self.counters.closing.store(true, Ordering::Release);
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publisher_defaults_enable_loopback_and_lan_and_preserve_explicit_overrides() -> Result<()> {
        let default = ZmqSettings::parse(Some("9872"), None, None)?
            .ok_or_else(|| Error::Config("default publisher unexpectedly disabled".into()))?;
        assert_eq!(default.endpoint, "tcp://0.0.0.0:9872");
        assert_eq!(
            ZmqSettings::parse(None, None, None)?
                .ok_or_else(|| Error::Config("missing default publisher".into()))?
                .endpoint,
            "tcp://0.0.0.0:9871"
        );
        for (host, endpoint) in [
            ("localhost", "tcp://127.0.0.1:9872"),
            ("10.126.17.125", "tcp://10.126.17.125:9872"),
            ("0.0.0.0", "tcp://0.0.0.0:9872"),
            ("*", "tcp://0.0.0.0:9872"),
            ("::1", "tcp://[::1]:9872"),
            ("2001:db8::125", "tcp://[2001:db8::125]:9872"),
            ("::", "tcp://[::]:9872"),
        ] {
            let settings = ZmqSettings::parse(Some("9872"), Some(host), Some("32"))?
                .ok_or_else(|| Error::Config("explicit publisher unexpectedly disabled".into()))?;
            assert_eq!(settings.endpoint, endpoint);
            assert_eq!(settings.capacity, 32);
        }
        for host in [
            "tracker.example.invalid",
            "tcp://127.0.0.1",
            "127.0.0.1:9872",
        ] {
            assert!(ZmqSettings::parse(Some("9872"), Some(host), None).is_err());
            assert!(ZmqSettings::parse(Some("0"), Some(host), None).is_err());
        }
        for port in ["-1", "0"] {
            assert!(ZmqSettings::parse(Some(port), Some("*"), None)?.is_none());
        }
        Ok(())
    }
    #[test]
    fn bounded_queue_overload_is_reported_without_waiting() -> Result<()> {
        let settings = ZmqSettings {
            endpoint: "tcp://127.0.0.1:9871".into(),
            capacity: 1,
        };
        let (sender, _receiver) = queue::channel(1);
        let counters = Arc::new(Counters::default());
        let mut output = ZmqOutput {
            settings,
            sender: Some(sender),
            counters,
            worker: None,
            reported_failures: 0,
        };
        assert!(output.enqueue(vec![b"first".to_vec()]).is_empty());
        assert_eq!(output.enqueue(vec![b"second".to_vec()]).len(), 1);
        let status = output.status();
        assert_eq!(status.enqueued, 1);
        assert_eq!(status.dropped, 1);
        assert_eq!(status.queue_len, 1);
        Ok(())
    }
}
