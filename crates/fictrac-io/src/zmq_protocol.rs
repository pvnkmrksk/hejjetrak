//! Narrow loopback PUB transport: ZMTP 2.0 and NULL-authenticated ZMTP 3.0.
//! The staged greeting is essential for NetMQ 4.0.0.1, which speaks ZMTP 2.0.
//! This is not a general ZeroMQ implementation: only SUB/XSUB peers are accepted.
use super::{CLOSE_DRAIN, Counters, MAX_WIRE_BYTES};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc as queue,
    task::JoinSet,
};

const MAX_PEERS: usize = 64;
const PEER_QUEUE: usize = 16;
const MAX_CONTROL: usize = 4096;
const MAX_PREFIX: usize = 256;
const MAX_SUBSCRIPTIONS: usize = 64;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(2);
const SEND_TIMEOUT: Duration = Duration::from_millis(25);
type Packet = Arc<Vec<Vec<u8>>>;
struct Peer {
    sender: queue::Sender<Packet>,
    subscribed: Arc<AtomicBool>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) async fn publish_loop(
    endpoint: String,
    mut receiver: queue::Receiver<Vec<Vec<u8>>>,
    counters: Arc<Counters>,
    ready: mpsc::SyncSender<std::result::Result<(), String>>,
) {
    let Some(address) = endpoint.strip_prefix("tcp://") else {
        let _ = ready.send(Err("ZMQ PUB requires a tcp:// loopback endpoint".into()));
        return;
    };
    let listener = match tokio::time::timeout(
        Duration::from_millis(500),
        TcpListener::bind(address),
    )
    .await
    {
        Ok(Ok(listener)) => listener,
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
    };
    counters.alive.store(true, Ordering::Release);
    let _ = ready.send(Ok(()));
    let mut peers: Vec<Peer> = Vec::new();
    let mut tasks = JoinSet::new();
    let mut deadline = None;
    loop {
        if counters.closing.load(Ordering::Acquire) && deadline.is_none() {
            deadline = Some(Instant::now() + CLOSE_DRAIN);
        }
        if deadline.is_some_and(|end| Instant::now() >= end) {
            break;
        }
        tokio::select! {
            packet = receiver.recv() => {
                let Some(packet) = packet else { break; };
                let packet = Arc::new(packet);
                peers.retain(|peer| !peer.sender.is_closed());
                for peer in &peers {
                    if peer.subscribed.load(Ordering::Acquire) && matches!(peer.sender.try_send(packet.clone()), Err(queue::error::TrySendError::Full(_))) {
                        counters.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
                // A PUB send attempt is complete even with no subscribers. It is
                // not a delivery acknowledgement, and no measurements are replayed.
                counters.published.fetch_add(1, Ordering::Relaxed);
            }
            incoming = listener.accept(), if deadline.is_none() => {
                match incoming {
                    Ok((stream, _)) => {
                        peers.retain(|peer| !peer.sender.is_closed());
                        if tasks.len() >= MAX_PEERS {
                            counters.fail("ZMQ subscriber limit reached (64 peers, including pending handshakes); excess connection closed".into());
                            continue;
                        }
                        let (sender, receiver) = queue::channel(PEER_QUEUE);
                        let subscribed = Arc::new(AtomicBool::new(false));
                        peers.push(Peer { sender, subscribed: subscribed.clone() });
                        let peer_counters = counters.clone();
                        tasks.spawn(async move {
                            if let Err(error) = serve_peer(stream, receiver, subscribed).await
                                && matches!(error.kind(), io::ErrorKind::InvalidData | io::ErrorKind::TimedOut)
                            {
                                peer_counters.fail(format!("ZMQ subscriber disconnected: {error}; other subscribers and tracking continue"));
                            }
                        });
                    }
                    Err(error) => {
                        counters.fail(format!("ZMQ PUB {endpoint} could not accept a subscriber: {error}; reapply output settings to restart"));
                        break;
                    }
                }
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {
                peers.retain(|peer| !peer.sender.is_closed());
            }
        }
    }
    receiver.close();
    let mut discarded = 0;
    while receiver.try_recv().is_ok() {
        discarded += 1;
    }
    counters.dropped.fetch_add(discarded, Ordering::Relaxed);
    peers.clear();
    if tokio::time::timeout(CLOSE_DRAIN, async {
        while tasks.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        tasks.abort_all();
    }
}

async fn handshake(stream: &mut TcpStream) -> io::Result<()> {
    // Send only the signature + major version. Sending all 64 bytes before
    // negotiation causes old NetMQ to interpret the minor byte as socket type.
    stream
        .write_all(&[0xff, 0, 0, 0, 0, 0, 0, 0, 1, 0x7f, 3])
        .await?;
    let mut greeting = [0; 11];
    stream.read_exact(&mut greeting).await?;
    if greeting[0] != 0xff || greeting[9] != 0x7f {
        return Err(invalid("invalid ZMTP greeting signature"));
    }
    match greeting[10] {
        1 | 2 => {
            // ZMTP/2.0 revision 1: socket type PUB (1), empty identity frame.
            stream.write_all(&[1, 0, 0]).await?;
            let socket_type = stream.read_u8().await?;
            if socket_type != 2 && socket_type != 10 {
                return Err(invalid("ZMTP/2 PUB accepts SUB or XSUB peers only"));
            }
            let identity = read_frame(stream).await?;
            if identity.flags != 0 || identity.body.len() > 255 {
                return Err(invalid("invalid ZMTP/2 identity frame"));
            }
        }
        3.. => {
            // RFC 23 requires accepting higher peer versions and negotiating
            // our own 3.0 protocol; it does not enable future protocol features.
            let mut remainder = [0; 53];
            remainder[1..5].copy_from_slice(b"NULL");
            stream.write_all(&remainder).await?;
            stream.read_exact(&mut remainder).await?;
            if remainder[1..5] != *b"NULL" || remainder[5..21].iter().any(|byte| *byte != 0) {
                return Err(invalid("ZMQ loopback PUB supports the NULL mechanism only"));
            }
            let mut properties = vec![11];
            properties.extend_from_slice(b"Socket-Type");
            properties.extend_from_slice(&3u32.to_be_bytes());
            properties.extend_from_slice(b"PUB");
            write_command(stream, b"READY", &properties).await?;
            let ready = read_frame(stream).await?;
            if ready.flags != 4 || !valid_ready(&ready.body) {
                return Err(invalid(
                    "ZMTP/3 PUB requires READY with SUB or XSUB Socket-Type",
                ));
            }
        }
        _ => {
            return Err(invalid(
                "unsupported ZMTP version; revision 1 (ZMTP/2.0) or ZMTP/3.x is required",
            ));
        }
    }
    Ok(())
}

fn valid_ready(body: &[u8]) -> bool {
    if body.get(..6) != Some(b"\x05READY") {
        return false;
    }
    let mut remaining = &body[6..];
    let mut socket_type = None;
    while let Some((&name_len, tail)) = remaining.split_first() {
        let name_len = usize::from(name_len);
        if name_len == 0 || tail.len() < name_len + 4 {
            return false;
        }
        let (name, rest) = tail.split_at(name_len);
        let len = u32::from_be_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
        if len > rest.len() - 4 {
            return false;
        }
        let (value, tail) = rest[4..].split_at(len);
        if name.eq_ignore_ascii_case(b"Socket-Type") {
            if socket_type.is_some() {
                return false;
            }
            socket_type = Some(value);
        }
        remaining = tail;
    }
    matches!(socket_type, Some(b"SUB" | b"XSUB"))
}

struct Frame {
    flags: u8,
    body: Vec<u8>,
}
async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Frame> {
    let flags = reader.read_u8().await?;
    if flags & !7 != 0 || flags & 5 == 5 {
        return Err(invalid("invalid ZMTP frame flags"));
    }
    let length = if flags & 2 != 0 {
        reader.read_u64().await?
    } else {
        u64::from(reader.read_u8().await?)
    };
    if length > MAX_CONTROL as u64 {
        return Err(invalid("ZMTP control frame exceeds 4096 bytes"));
    }
    let length = length as usize;
    let mut body = Vec::new();
    body.try_reserve_exact(length).map_err(io::Error::other)?;
    body.resize(length, 0);
    reader.read_exact(&mut body).await?;
    Ok(Frame {
        flags: flags & !2,
        body,
    })
}

enum Control {
    Subscribe(Vec<u8>),
    Cancel(Vec<u8>),
    Ping(Vec<u8>),
}
async fn read_controls(
    mut reader: tokio::net::tcp::OwnedReadHalf,
    updates: queue::Sender<Control>,
) -> io::Result<()> {
    loop {
        let frame = read_frame(&mut reader).await?;
        let control = if frame.flags == 0 {
            match frame.body.split_first() {
                Some((&1, prefix)) if prefix.len() <= MAX_PREFIX => {
                    Control::Subscribe(prefix.to_vec())
                }
                Some((&0, prefix)) if prefix.len() <= MAX_PREFIX => {
                    Control::Cancel(prefix.to_vec())
                }
                _ => {
                    return Err(invalid(
                        "invalid ZMTP subscription or prefix over 256 bytes",
                    ));
                }
            }
        } else if frame.flags == 4 {
            match frame.body.split_first() {
                Some((&9, body))
                    if body.starts_with(b"SUBSCRIBE") && body.len() - 9 <= MAX_PREFIX =>
                {
                    Control::Subscribe(body[9..].to_vec())
                }
                Some((&6, body)) if body.starts_with(b"CANCEL") && body.len() - 6 <= MAX_PREFIX => {
                    Control::Cancel(body[6..].to_vec())
                }
                Some((&4, body)) if body.starts_with(b"PING") && (6..=22).contains(&body.len()) => {
                    Control::Ping(body[6..].to_vec())
                }
                _ => return Err(invalid("unsupported or malformed ZMTP command")),
            }
        } else {
            return Err(invalid("SUB/XSUB control frames must not be multipart"));
        };
        if updates.send(control).await.is_err() {
            return Ok(());
        }
    }
}

async fn serve_peer(
    mut stream: TcpStream,
    mut outbound: queue::Receiver<Packet>,
    subscribed: Arc<AtomicBool>,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    tokio::time::timeout(HANDSHAKE_TIMEOUT, handshake(&mut stream))
        .await
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                "ZMTP handshake exceeded two seconds",
            )
        })??;
    let (reader, mut writer) = stream.into_split();
    let (updates, mut controls) = queue::channel(16);
    // A separate reader retains partial frames while the writer publishes.
    // Cancelling read_exact inside select would lose fragmented control bytes.
    let reader = tokio::spawn(read_controls(reader, updates));
    let mut subscriptions: Vec<Vec<u8>> = Vec::new();
    let outcome = async {
        loop {
            tokio::select! {
                biased;
                control = controls.recv() => {
                    match control {
                        Some(Control::Subscribe(prefix)) => {
                            if !subscriptions.contains(&prefix) {
                                if subscriptions.len() == MAX_SUBSCRIPTIONS { return Err(invalid("ZMQ subscriber exceeds 64 prefixes")); }
                                if subscriptions.is_empty() {
                                    // A new subscription does not replay an older scene.
                                    for _ in 0..PEER_QUEUE { if outbound.try_recv().is_err() { break; } }
                                }
                                subscriptions.push(prefix);
                                subscribed.store(true, Ordering::Release);
                            }
                        }
                        Some(Control::Cancel(prefix)) => {
                            subscriptions.retain(|candidate| *candidate != prefix);
                            subscribed.store(!subscriptions.is_empty(), Ordering::Release);
                        },
                        Some(Control::Ping(context)) => {
                            tokio::time::timeout(SEND_TIMEOUT, write_command(&mut writer, b"PONG", &context)).await
                                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "ZMQ subscriber PONG write exceeded 25 ms"))??;
                        }
                        None => break,
                    }
                }
                packet = outbound.recv() => {
                    let Some(packet) = packet else { break; };
                    let Some(first) = packet.first() else { continue; };
                    if subscriptions.iter().any(|prefix| first.starts_with(prefix)) {
                        let encoded = encode_packet(&packet)?;
                        tokio::time::timeout(SEND_TIMEOUT, writer.write_all(&encoded)).await
                            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "ZMQ subscriber write exceeded 25 ms"))??;
                    }
                }
            }
        }
        Ok(())
    }.await;
    subscribed.store(false, Ordering::Release);
    let read_outcome = if reader.is_finished() {
        reader.await.map_err(io::Error::other)?
    } else {
        reader.abort();
        let _ = reader.await;
        Ok(())
    };
    // Any cancelled partial write closes this peer; its stream is never reused.
    outcome.and(read_outcome)
}

fn encode_packet(packet: &[Vec<u8>]) -> io::Result<Vec<u8>> {
    let total = packet
        .iter()
        .try_fold(0usize, |total, frame| total.checked_add(frame.len()));
    let Some(total) = total.filter(|size| *size <= MAX_WIRE_BYTES) else {
        return Err(invalid("ZMQ message exceeds 16 KiB"));
    };
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(total + 18)
        .map_err(io::Error::other)?;
    for (index, frame) in packet.iter().enumerate() {
        let more = u8::from(index + 1 < packet.len());
        if frame.len() <= 255 {
            bytes.extend_from_slice(&[more, frame.len() as u8]);
        } else {
            bytes.push(more | 2);
            bytes.extend_from_slice(&(frame.len() as u64).to_be_bytes());
        }
        bytes.extend_from_slice(frame);
    }
    Ok(bytes)
}

async fn write_command(
    writer: &mut (impl AsyncWrite + Unpin),
    name: &[u8],
    body: &[u8],
) -> io::Result<()> {
    let length = 1 + name.len() + body.len();
    if name.len() > 255 || length > MAX_CONTROL {
        return Err(invalid("invalid ZMTP command size"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length + 9)
        .map_err(io::Error::other)?;
    if length <= 255 {
        bytes.extend_from_slice(&[4, length as u8]);
    } else {
        bytes.push(6);
        bytes.extend_from_slice(&(length as u64).to_be_bytes());
    }
    bytes.push(name.len() as u8);
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(body);
    writer.write_all(&bytes).await
}
