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
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{self as queue, Sender};
use zeromq::{PubSocket, Socket, SocketSend};
const DEFAULT_CAPACITY: usize = 256;
const MAX_CAPACITY: usize = 4096;
const MAX_WIRE_BYTES: usize = 16_384;
const SEND_TIMEOUT: Duration = Duration::from_millis(25);
const CLOSE_DRAIN: Duration = Duration::from_millis(50);

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
            .map(str::parse::<i32>)
            .transpose()
            .map_err(|_| {
                Error::Config("zmq_port must be an integer: -1/0 disables, 1..65535 enables".into())
            })?
            .unwrap_or(-1);
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
        let host = host.unwrap_or("127.0.0.1");
        let address = if host.eq_ignore_ascii_case("localhost") {
            IpAddr::from([127, 0, 0, 1])
        } else {
            host.parse::<IpAddr>().map_err(|_| {
                Error::Config("zmq_host must be localhost or a loopback IP address".into())
            })?
        };
        if !address.is_loopback() {
            return Err(Error::Config(
                "zmq_host must be a loopback address (for example 127.0.0.1 or ::1)".into(),
            ));
        }
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
    sender: Option<Sender<Vec<u8>>>,
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
                runtime.block_on(publish_loop(endpoint,receiver,worker_counters.clone(),ready_tx));
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
    pub fn enqueue(&mut self, packet: Vec<u8>) -> Vec<String> {
        let mut errors = self.take_errors();
        if packet.len() > MAX_WIRE_BYTES {
            self.counters.dropped.fetch_add(1, Ordering::Relaxed);
            errors.push("ZMQ row exceeds its 16 KiB wire limit".into());
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
async fn publish_loop(
    endpoint: String,
    mut receiver: queue::Receiver<Vec<u8>>,
    counters: Arc<Counters>,
    ready: mpsc::SyncSender<std::result::Result<(), String>>,
) {
    let mut publisher = PubSocket::new();
    match tokio::time::timeout(Duration::from_millis(500), publisher.bind(&endpoint)).await {
        Ok(Ok(_)) => {
            counters.alive.store(true, Ordering::Release);
            let _ = ready.send(Ok(()));
        }
        result => {
            let detail = match result {
                Ok(Err(error)) => error.to_string(),
                Err(_) => "bind timed out".into(),
                Ok(Ok(_)) => String::new(),
            };
            let message = format!(
                "cannot bind ZMQ PUB {endpoint}: {detail}; use a free zmq_port for each instance"
            );
            counters.fail(message.clone());
            let _ = ready.send(Err(message));
            return;
        }
    }
    let mut close_deadline = None;
    while let Some(packet) = receiver.recv().await {
        if counters.closing.load(Ordering::Acquire) && close_deadline.is_none() {
            close_deadline = Some(Instant::now() + CLOSE_DRAIN);
        }
        let remaining =
            close_deadline.map(|end: Instant| end.saturating_duration_since(Instant::now()));
        if remaining.is_some_and(|time| time.is_zero()) {
            counters.dropped.fetch_add(1, Ordering::Relaxed);
            break;
        }
        let timeout = remaining.map_or(SEND_TIMEOUT, |time| time.min(SEND_TIMEOUT));
        match tokio::time::timeout(timeout, publisher.send(packet.into())).await {
            Ok(Ok(())) => {
                counters.published.fetch_add(1, Ordering::Relaxed);
            }
            result => {
                let detail = match result {
                    Ok(Err(error)) => error.to_string(),
                    Err(_) => "subscriber backpressure exceeded 25 ms".into(),
                    Ok(Ok(())) => String::new(),
                };
                counters.dropped.fetch_add(1, Ordering::Relaxed);
                counters.fail(format!("ZMQ PUB {endpoint} stopped: {detail}; tracking continues; reapply output configuration to restart"));
                break;
            }
        }
    }
    receiver.close();
    let mut discarded = 0;
    while receiver.try_recv().is_ok() {
        discarded += 1;
    }
    counters.dropped.fetch_add(discarded, Ordering::Relaxed);
    // Drop the whole socket after cancelling a send, so no partially written
    // ZMTP frame is reused. PUB delivery remains best effort, without replay.
}
#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(output.enqueue(b"first".to_vec()).is_empty());
        assert_eq!(output.enqueue(b"second".to_vec()).len(), 1);
        let status = output.status();
        assert_eq!(status.enqueued, 1);
        assert_eq!(status.dropped, 1);
        assert_eq!(status.queue_len, 1);
        Ok(())
    }
}
