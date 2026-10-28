use fictrac_core::TrackRow;
use fictrac_io::{ConfigDocument, OutputSinks, load_config_text, write_dat};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::{Mutex, MutexGuard},
    time::{Duration, Instant},
};
use zeromq::{Socket, SocketRecv};
type TestResult = Result<(), Box<dyn std::error::Error>>;
// Port probes release their temporary listener before the publisher binds.
// Serialize this binary's socket fixtures so another test cannot take that
// port in between, including as an outgoing subscriber's ephemeral port.
static SOCKET_TESTS: Mutex<()> = Mutex::new(());
fn socket_test() -> Result<MutexGuard<'static, ()>, Box<dyn std::error::Error>> {
    SOCKET_TESTS
        .lock()
        .map_err(|_| "socket test lock poisoned".into())
}
fn port() -> Result<u16, Box<dyn std::error::Error>> {
    // Reserve the publisher's actual wildcard endpoint: BSD/macOS can allow
    // a loopback listener and a reusable wildcard listener on the same port.
    let socket = TcpListener::bind("0.0.0.0:0")?;
    Ok(socket.local_addr()?.port())
}
fn row() -> TrackRow {
    TrackRow {
        frame: 4,
        sequence: 7,
        delta_camera: [0.01, 0.02, 0.03],
        error: 10.,
        delta_lab: [0.01, 0.02, 0.03],
        absolute_camera: [0.; 3],
        absolute_lab: [0.; 3],
        position: [0.1, 0.2],
        heading: 0.3,
        direction: 0.4,
        speed: 0.5,
        integrated: [0.1, 0.2],
        timestamp_ms: 100.,
        delta_ms: 33.33,
        accepted: true,
        status: "tracked",
        samples: 100,
    }
}
#[test]
fn real_rust_subscriber_and_udp_receive_identical_full_rows() -> TestResult {
    let _socket_test = socket_test()?;
    let zmq_port = port()?;
    let udp = UdpSocket::bind("127.0.0.1:0")?;
    udp.set_read_timeout(Some(Duration::from_millis(100)))?;
    let document = ConfigDocument::parse(format!(
        "sock_host:127.0.0.1\nsock_port:{}\nzmq_host:localhost\nzmq_port:{zmq_port}\nzmq_queue_capacity:8\n",
        udp.local_addr()?.port()
    ))?;
    let mut sinks = OutputSinks::from_config(&document)?;
    // An old client that stalls halfway through greeting must not block a
    // healthy modern subscriber or numerical publication.
    let mut half_handshake = TcpStream::connect(("127.0.0.1", zmq_port))?;
    half_handshake.write_all(&[0xff])?;
    assert_eq!(
        sinks.status().udp_destination,
        Some(udp.local_addr()?.to_string())
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut subscriber = zeromq::SubSocket::new();
        subscriber
            .connect(&format!("tcp://127.0.0.1:{zmq_port}"))
            .await?;
        subscriber.subscribe("FT, ").await?;
        let mut expected = b"FT, ".to_vec();
        write_dat(&mut expected, &row())?;
        let started = Instant::now();
        let mut received = false;
        while started.elapsed() < Duration::from_secs(2) {
            assert!(sinks.write(&row()).is_empty());
            let mut packet = [0; 4096];
            let n = udp.recv(&mut packet)?;
            assert_eq!(&packet[..n], expected.as_slice());
            if let Ok(message) =
                tokio::time::timeout(Duration::from_millis(20), subscriber.recv()).await
            {
                let message = message?;
                assert_eq!(message.len(), 1);
                let frame = message.get(0).ok_or("empty ZMQ message")?;
                assert_eq!(frame.as_ref(), expected.as_slice());
                received = true;
                break;
            }
        }
        assert!(received, "subscriber must receive a compatible ZMTP FT row");
        let before = sinks.status().zmq.ok_or("ZMQ status missing")?;
        assert!(before.enqueued > 0);
        assert!(before.worker_alive);
        assert_eq!(before.dropped, 0);
        let mut rejected = row();
        rejected.accepted = false;
        assert!(sinks.write(&rejected).is_empty());
        assert_eq!(
            sinks.status().zmq.ok_or("ZMQ status missing")?.enqueued,
            before.enqueued
        );
        let value = serde_json::to_value(sinks.status())?;
        assert_eq!(value["udp_enabled"], true);
        assert_eq!(value["zmq"]["queue_capacity"], 8);
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}

#[test]
fn explicit_wildcard_bind_serves_modern_and_legacy_subscribers_with_unchanged_bytes() -> TestResult
{
    let _socket_test = socket_test()?;
    for (host, mode, prefix) in [
        ("", "native_ft", "FT, "),
        ("0.0.0.0", "native_ft", "FT, "),
        ("*", "legacy_matrex_vr", "{"),
    ] {
        let reservation = TcpListener::bind("0.0.0.0:0")?;
        let port = reservation.local_addr()?.port();
        drop(reservation);
        let host_line = if host.is_empty() {
            String::new()
        } else {
            format!("zmq_host:{host}\n")
        };
        let document = ConfigDocument::parse(format!(
            "{host_line}zmq_port:{port}\nzmq_output_mode:{mode}\n"
        ))?;
        let mut sinks = OutputSinks::from_config(&document)?;
        assert_eq!(
            sinks
                .status()
                .zmq
                .ok_or("missing wildcard publisher")?
                .endpoint,
            format!("tcp://0.0.0.0:{port}")
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let mut subscriber = zeromq::SubSocket::new();
            subscriber.connect(&format!("tcp://127.0.0.1:{port}")).await?;
            subscriber.subscribe(prefix).await?;
            let expected = if mode == "native_ft" {
                let mut native = b"FT, ".to_vec();
                write_dat(&mut native, &row())?;
                native
            } else {
                // Python json.dumps of the original bridge's selected FT
                // values, including its field order and radians-to-degrees.
                br#"{"x": 0.1, "y": 0.2, "z": 0.0, "yaw": 17.188733853924695, "pitch": 0.0, "roll": 0.0}"#.to_vec()
            };
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                assert!(sinks.write(&row()).is_empty());
                if let Ok(message) =
                    tokio::time::timeout(Duration::from_millis(20), subscriber.recv()).await
                {
                    let message = message?;
                    assert_eq!(message.len(), 1);
                    assert_eq!(message.get(0).ok_or("empty modern packet")?.as_ref(), expected);
                    break;
                }
                assert!(Instant::now() < deadline, "wildcard publisher must reach a subscriber");
            }
            // Binding scope must not change the old ZMTP 2.0 protocol or wire
            // bytes. This socket subscribes after the independent modern peer.
            let mut legacy = zmtp2(port, true)?;
            subscription(&mut legacy, prefix.as_bytes(), true)?;
            let (flags, packet) = receive_published(&mut sinks, &mut legacy)?;
            assert_eq!(flags & 1, 0);
            assert_eq!(packet, expected);
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;
    }
    Ok(())
}

fn zmtp2(port: u16, fragmented: bool) -> Result<TcpStream, Box<dyn std::error::Error>> {
    let mut socket = TcpStream::connect(("127.0.0.1", port))?;
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    socket.set_write_timeout(Some(Duration::from_secs(1)))?;
    socket.set_nodelay(true)?;
    let mut signature = [0; 11];
    socket.read_exact(&mut signature)?;
    assert_eq!(signature[0], 0xff);
    assert_eq!(signature[9], 0x7f);
    assert_eq!(signature[10], 3);
    let greeting = [0xff, 0, 0, 0, 0, 0, 0, 0, 1, 0x7f, 1];
    if fragmented {
        for byte in greeting {
            socket.write_all(&[byte])?;
        }
    } else {
        socket.write_all(&greeting)?;
    }
    let mut rest = [0; 3];
    socket.read_exact(&mut rest)?;
    assert_eq!(rest, [1, 0, 0], "PUB socket type and empty identity");
    socket.write_all(&[2, 0, 0])?; // SUB socket type and empty identity.
    Ok(socket)
}
fn subscription(socket: &mut TcpStream, prefix: &[u8], subscribe: bool) -> TestResult {
    let length = u8::try_from(prefix.len() + 1)?;
    socket.write_all(&[0, length, u8::from(subscribe)])?;
    socket.write_all(prefix)?;
    Ok(())
}
fn wire_frame(socket: &mut TcpStream) -> Result<(u8, Vec<u8>), Box<dyn std::error::Error>> {
    let mut header = [0; 2];
    socket.read_exact(&mut header)?;
    let length = if header[0] & 2 == 0 {
        usize::from(header[1])
    } else {
        let mut bytes = [0; 8];
        bytes[0] = header[1];
        socket.read_exact(&mut bytes[1..])?;
        usize::try_from(u64::from_be_bytes(bytes))?
    };
    if length > 16384 {
        return Err("test received an oversized frame".into());
    }
    let mut body = vec![0; length];
    socket.read_exact(&mut body)?;
    Ok((header[0], body))
}
fn receive_published(
    sinks: &mut OutputSinks,
    socket: &mut TcpStream,
) -> Result<(u8, Vec<u8>), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let _ = sinks.write(&row());
        let mut peek = [0];
        match socket.peek(&mut peek) {
            Ok(n) if n > 0 => return wire_frame(socket),
            Ok(_) => return Err("publisher closed unexpectedly".into()),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(error) => return Err(error.into()),
        }
        if Instant::now() > deadline {
            return Err("no published row received".into());
        }
    }
}

#[test]
fn fragmented_zmtp2_greeting_and_reconnect_receive_native_ft_unchanged() -> TestResult {
    let _socket_test = socket_test()?;
    let port = port()?;
    let doc = ConfigDocument::parse(format!("zmq_port:{port}\n"))?;
    let mut sinks = OutputSinks::from_config(&doc)?;
    let mut expected = b"FT, ".to_vec();
    write_dat(&mut expected, &row())?;
    for _ in 0..3 {
        let mut socket = zmtp2(port, true)?;
        subscription(&mut socket, b"FT, ", true)?;
        let (flags, frame) = receive_published(&mut sinks, &mut socket)?;
        assert_eq!(flags & 1, 0);
        assert_eq!(frame, expected);
    }
    assert!(sinks.status().zmq.ok_or("missing publisher")?.worker_alive);
    Ok(())
}

#[test]
fn zmtp2_multipart_is_atomic_and_prefix_filters_the_topic() -> TestResult {
    let _socket_test = socket_test()?;
    let port = port()?;
    let doc = ConfigDocument::parse(format!(
        "zmq_port:{port}\nzmq_output_mode:legacy_matrex_vr\nzmq_framing:topic_json\nzmq_topic:legacy/test\n"
    ))?;
    let mut sinks = OutputSinks::from_config(&doc)?;
    let mut socket = zmtp2(port, false)?;
    subscription(&mut socket, b"legacy/", true)?;
    let (flags, topic) = receive_published(&mut sinks, &mut socket)?;
    assert_eq!(flags & 1, 1);
    assert_eq!(topic, b"legacy/test");
    let (flags, json) = wire_frame(&mut socket)?;
    assert_eq!(flags & 1, 0);
    let data: serde_json::Value = serde_json::from_slice(&json)?;
    assert_eq!(data["x"], 0.1);
    assert_eq!(data["z"], 0.0);
    assert!(data["yaw"].as_f64().ok_or("missing yaw")?.is_finite());
    Ok(())
}

#[test]
fn oversized_peer_control_is_observable_and_other_subscribers_continue() -> TestResult {
    let _socket_test = socket_test()?;
    let port = port()?;
    let doc = ConfigDocument::parse(format!("zmq_port:{port}\n"))?;
    let mut sinks = OutputSinks::from_config(&doc)?;
    let mut bad = zmtp2(port, false)?;
    bad.write_all(&[2])?;
    bad.write_all(&u64::MAX.to_be_bytes())?;
    let mut byte = [0];
    assert_eq!(
        bad.read(&mut byte)?,
        0,
        "oversize rejected before allocation"
    );
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if sinks.status().zmq.ok_or("missing publisher")?.failures > 0 {
            break;
        }
        if Instant::now() > deadline {
            return Err("peer error was not reported".into());
        }
        std::thread::yield_now();
    }
    let mut healthy = zmtp2(port, true)?;
    subscription(&mut healthy, b"FT", true)?;
    assert!(
        receive_published(&mut sinks, &mut healthy)?
            .1
            .starts_with(b"FT, ")
    );
    assert!(sinks.status().zmq.ok_or("missing publisher")?.worker_alive);
    Ok(())
}

fn wait_published(sinks: &OutputSinks, expected: u64) -> TestResult {
    let deadline = Instant::now() + Duration::from_secs(2);
    while sinks
        .status()
        .zmq
        .as_ref()
        .ok_or("missing publisher")?
        .published
        < expected
    {
        if Instant::now() > deadline {
            return Err("publisher did not drain its application queue".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn no_frame(socket: &mut TcpStream) -> TestResult {
    let mut byte = [0];
    let result = socket.read(&mut byte);
    assert!(
        matches!(result, Err(ref error) if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)),
        "unsubscribed measurements were replayed: {result:?}"
    );
    Ok(())
}

#[test]
fn no_measurements_replay_after_handshake_or_empty_prefix_resubscribe() -> TestResult {
    let _socket_test = socket_test()?;
    let port = port()?;
    let doc = ConfigDocument::parse(format!("zmq_port:{port}\n"))?;
    let mut sinks = OutputSinks::from_config(&doc)?;
    let mut socket = zmtp2(port, true)?;
    // These rows complete publication before the peer has ever subscribed.
    for _ in 0..20 {
        assert!(sinks.write(&row()).is_empty());
    }
    wait_published(&sinks, 20)?;
    subscription(&mut socket, b"", true)?;
    no_frame(&mut socket)?;
    assert!(
        receive_published(&mut sinks, &mut socket)?
            .1
            .starts_with(b"FT, ")
    );
    subscription(&mut socket, b"", false)?;
    no_frame(&mut socket)?;
    let before = sinks.status().zmq.ok_or("missing publisher")?.enqueued;
    for _ in 0..20 {
        assert!(sinks.write(&row()).is_empty());
    }
    wait_published(&sinks, before + 20)?;
    subscription(&mut socket, b"", true)?;
    no_frame(&mut socket)?;
    assert!(
        receive_published(&mut sinks, &mut socket)?
            .1
            .starts_with(b"FT, ")
    );
    Ok(())
}
#[test]
fn occupied_pub_port_is_actionable_and_disabled_pub_opens_no_socket() -> TestResult {
    let _socket_test = socket_test()?;
    let occupied = TcpListener::bind("0.0.0.0:0")?;
    let port = occupied.local_addr()?.port();
    let doc = ConfigDocument::parse(format!("zmq_port:{port}\n"))?;
    let error = match OutputSinks::from_config(&doc) {
        Ok(_) => return Err("occupied port unexpectedly bound".into()),
        Err(error) => error,
    };
    assert!(error.to_string().contains("cannot bind ZMQ PUB"));
    assert!(error.to_string().contains("zmq_port must be unused"));
    let disabled = ConfigDocument::parse("zmq_port:-1\n".into())?;
    let sinks = OutputSinks::from_config(&disabled)?;
    assert!(!sinks.active());
    assert!(sinks.status().zmq.is_none());
    Ok(())
}
#[test]
fn reconfigure_reuses_healthy_outputs_and_restores_previous_pub_after_busy_port() -> TestResult {
    let _socket_test = socket_test()?;
    let old_port = port()?;
    let mut old = ConfigDocument::parse(format!("zmq_port:{old_port}\n"))?;
    let mut sinks = OutputSinks::from_config(&old)?;
    assert!(sinks.write(&row()).is_empty());
    sinks.reconfigure(&old, &old)?;
    assert_eq!(sinks.status().zmq.ok_or("missing publisher")?.enqueued, 1);

    let busy = TcpListener::bind("0.0.0.0:0")?;
    let mut replacement = ConfigDocument::parse(old.text().into())?;
    replacement.set("zmq_port", &busy.local_addr()?.port().to_string())?;
    let error = sinks
        .reconfigure(&old, &replacement)
        .err()
        .ok_or("busy replacement unexpectedly succeeded")?;
    assert!(
        error
            .to_string()
            .contains("previous output settings restored")
    );
    let restored = sinks.status().zmq.ok_or("publisher was lost on rollback")?;
    assert_eq!(restored.endpoint, format!("tcp://0.0.0.0:{old_port}"));
    assert!(restored.worker_alive);
    assert_eq!(restored.failures, 0);

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut subscriber = zeromq::SubSocket::new();
        subscriber.connect(&restored.endpoint).await?;
        subscriber.subscribe("FT, ").await?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            assert!(sinks.write(&row()).is_empty());
            if let Ok(message) =
                tokio::time::timeout(Duration::from_millis(20), subscriber.recv()).await
            {
                let message = message?;
                let mut expected = b"FT, ".to_vec();
                write_dat(&mut expected, &row())?;
                assert_eq!(message.get(0).ok_or("empty row")?.as_ref(), expected);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "restored publisher must send rows"
            );
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    old.set("zmq_port", "-1")?;
    sinks.reconfigure(
        &ConfigDocument::parse(format!("zmq_port:{old_port}\n"))?,
        &old,
    )?;
    assert!(!sinks.active());
    Ok(())
}
#[test]
fn zmq_parameters_validate_losslessly_without_entering_the_core() -> TestResult {
    let base = include_str!("../../../examples/upstream-config.txt");
    for (key, value) in [
        ("zmq_port", "65536"),
        ("zmq_port", "abc"),
        ("zmq_queue_capacity", "0"),
        ("zmq_queue_capacity", "4097"),
        ("zmq_host", "tracker.example.invalid"),
    ] {
        let mut doc = ConfigDocument::parse(base.into())?;
        doc.set(key, value)?;
        assert!(
            load_config_text(doc.text(), 384, 288).is_err(),
            "{key}={value} must fail"
        );
    }
    let mut doc = ConfigDocument::parse(base.into())?;
    doc.set("zmq_port", "9871")?;
    doc.set("zmq_host", "localhost")?;
    doc.set("zmq_queue_capacity", "32")?;
    let loaded = load_config_text(doc.text(), 384, 288)?;
    assert!(loaded.warnings.iter().all(|w| !w.contains("zmq_")));
    assert_eq!(ConfigDocument::parse(doc.text().into())?.text(), doc.text());
    Ok(())
}
#[test]
fn root_vr_configs_and_fallback_use_independent_ports_labels_and_sources() -> TestResult {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut document = ConfigDocument::parse(std::fs::read_to_string(root.join("VR1.txt"))?)?;
    let mut path = Some(root.join("VR1.txt"));
    for index in 1..=4 {
        assert_eq!(
            document.get("rust_instance_name"),
            Some(format!("VR{index}").as_str())
        );
        assert_eq!(
            document.get("src_fn"),
            Some((2 * (index - 1)).to_string().as_str())
        );
        assert_eq!(
            document.get("sock_port"),
            Some((1110 + index).to_string().as_str())
        );
        // Saved rigs retain their explicit direct publisher ports. The bridge
        // launcher disables those ports in copies before using republishers.
        assert_eq!(
            document.get("zmq_port"),
            Some((9870 + index).to_string().as_str())
        );
        assert_eq!(document.get("rust_camera_width"), Some("320"));
        assert_eq!(document.get("rust_camera_height"), Some("240"));
        assert_eq!(document.get("vfov"), Some("17"));
        load_config_text(document.text(), 320, 240)?;
        (document, path) = document.next_instance(path.as_deref())?;
    }
    // Saved rigs cycle rather than inventing a fifth metadata-only camera.
    assert_eq!(path, Some(root.join("VR1.txt")));
    assert_eq!(document.get("rust_instance_name"), Some("VR1"));
    assert_eq!(document.get("src_fn"), Some("0"));
    assert_eq!(document.get("sock_port"), Some("1111"));
    // An unsaved document still uses the generic incrementing fallback.
    let saved = ConfigDocument::parse(std::fs::read_to_string(root.join("VR4.txt"))?)?;
    let (mut document, path) = saved.next_instance(None)?;
    assert!(path.is_none());
    assert_eq!(document.get("rust_instance_name"), Some("VR5"));
    assert_eq!(document.get("src_fn"), Some("7"));
    assert_eq!(document.get("sock_port"), Some("1115"));
    assert_eq!(document.get("zmq_port"), Some("9875"));
    document.set("zmq_port", "0")?;
    let (disabled, _) = document.next_instance(None)?;
    assert_eq!(disabled.get("zmq_port"), Some("0"));
    // An explicitly enabled direct publisher still increments independently.
    document.set("zmq_port", "9875")?;
    document.set("src_fn", "sample.mp4")?;
    document.set("sock_port", "-1")?;
    let (copy, _) = document.next_instance(None)?;
    assert_eq!(copy.get("src_fn"), Some("sample.mp4"));
    assert_eq!(copy.get("sock_port"), Some("-1"));
    assert_eq!(copy.get("zmq_port"), Some("9876"));
    assert_eq!(copy.get("rust_instance_name"), Some("VR6"));
    document.set("zmq_port", "65535")?;
    assert!(document.next_instance(None).is_err());
    Ok(())
}

#[test]
fn implicit_publisher_port_is_incremented_when_copying_a_rig() -> TestResult {
    let original = "src_fn:0\n# Keep calibration and unknown fields intact.\n";
    let document = ConfigDocument::parse(original.into())?;
    let (copy, _) = document.next_instance(None)?;
    assert_eq!(copy.get("zmq_port"), Some("9872"));
    assert_eq!(copy.get("src_fn"), Some("1"));
    assert_eq!(document.text(), original);
    Ok(())
}
