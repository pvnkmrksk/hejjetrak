//! Payload/config regressions. Old Unity/NetMQ acceptance is verified separately.
use fictrac_core::TrackRow;
use fictrac_io::{
    ConfigDocument, OutputSinks, ZmqFraming, ZmqOutputMode, load_config_text, write_dat,
};
use std::{
    io::Write,
    net::{TcpListener, TcpStream, UdpSocket},
    time::{Duration, Instant},
};
use zeromq::{Socket, SocketRecv};
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn port() -> Result<u16, Box<dyn std::error::Error>> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}
fn row() -> TrackRow {
    TrackRow {
        frame: 4,
        sequence: 7,
        delta_camera: [1.0, 2.0, 3.0],
        error: 4.0,
        delta_lab: [5.0, 6.0, 7.0],
        absolute_camera: [8.0, 9.0, 10.0],
        absolute_lab: [11.0, 12.0, 13.0],
        position: [-12.5, 42.25],
        heading: 0.3,
        direction: 0.4,
        speed: 0.5,
        integrated: [19.0, 20.0],
        timestamp_ms: 100.0,
        delta_ms: 33.33,
        accepted: true,
        status: "tracked",
        samples: 100,
    }
}

#[test]
fn compatibility_fields_validate_losslessly_without_changing_the_engine() -> TestResult {
    let base = include_str!("../../../examples/upstream-config.txt");
    let baseline = serde_json::to_value(load_config_text(base, 384, 288)?.config)?;
    for (key, value) in [
        ("zmq_output_mode", "legacy".to_owned()),
        ("zmq_framing", "multipart".to_owned()),
        ("zmq_topic", "é".repeat(129)),
    ] {
        let mut document = ConfigDocument::parse(base.into())?;
        document.set(key, &value)?;
        assert!(
            load_config_text(document.text(), 384, 288).is_err(),
            "{key}"
        );
        assert!(OutputSinks::from_config(&document).is_err(), "{key}");
    }
    let mut document =
        ConfigDocument::parse(format!("{base}\n# Keep camera calibration intact.\n"))?;
    document.set("zmq_output_mode", "legacy_matrex_vr")?;
    document.set("zmq_framing", "topic_json")?;
    document.set("zmq_topic", "FT")?;
    let loaded = load_config_text(document.text(), 384, 288)?;
    assert_eq!(serde_json::to_value(loaded.config)?, baseline);
    assert!(
        loaded
            .warnings
            .iter()
            .all(|warning| !warning.contains("zmq_"))
    );
    assert!(document.text().starts_with(base));
    assert!(
        document
            .text()
            .contains("# Keep camera calibration intact.")
    );
    assert_eq!(
        ConfigDocument::parse(document.text().into())?.text(),
        document.text()
    );
    document.set("zmq_port", "9871")?;
    let (next, _) = document.next_instance(None)?;
    assert_eq!(next.get("zmq_port"), Some("9872"));
    assert_eq!(next.get("zmq_output_mode"), Some("legacy_matrex_vr"));
    assert_eq!(next.get("zmq_framing"), Some("topic_json"));
    assert_eq!(next.get("zmq_topic"), Some("FT"));
    Ok(())
}

#[test]
fn invalid_compatibility_edits_keep_the_existing_publisher_and_queue_counters() -> TestResult {
    let document = ConfigDocument::parse(format!(
        "zmq_port:{}\nzmq_output_mode:legacy_matrex_vr\nzmq_framing:single\n",
        port()?
    ))?;
    let mut sinks = OutputSinks::from_config(&document)?;
    assert!(sinks.write(&row()).is_empty());
    let before = sinks.status().zmq.ok_or("publisher missing")?;
    for (key, value) in [
        ("zmq_output_mode", "unknown".to_owned()),
        ("zmq_framing", "unknown".to_owned()),
        ("zmq_topic", "x".repeat(257)),
    ] {
        let mut invalid = document.clone();
        invalid.set(key, &value)?;
        assert!(sinks.reconfigure(&document, &invalid).is_err());
        let status = sinks.status();
        let transport = status.zmq.ok_or("publisher lost after invalid edit")?;
        assert_eq!(transport.endpoint, before.endpoint);
        assert_eq!(transport.enqueued, before.enqueued);
        assert_eq!(transport.failures, before.failures);
        assert!(transport.worker_alive);
        let payload = status.zmq_payload.ok_or("payload status missing")?;
        assert_eq!(payload.mode, ZmqOutputMode::LegacyMatrexVr);
        assert_eq!(payload.framing, ZmqFraming::Single);
    }
    Ok(())
}

#[test]
fn unrelated_edits_reuse_a_healthy_publisher_after_a_bad_peer() -> TestResult {
    let zmq_port = port()?;
    let document = ConfigDocument::parse(format!(
        "zmq_port:{zmq_port}\nzmq_output_mode:legacy_matrex_vr\n"
    ))?;
    let mut sinks = OutputSinks::from_config(&document)?;
    assert!(sinks.write(&row()).is_empty());
    let mut peer = TcpStream::connect(("127.0.0.1", zmq_port))?;
    peer.write_all(&[0; 11])?; // Invalid ZMTP signature: only this peer is closed.
    drop(peer);
    let deadline = Instant::now() + Duration::from_secs(2);
    let before = loop {
        let status = sinks.status().zmq.ok_or("publisher missing")?;
        if status.failures > 0 {
            break status;
        }
        assert!(Instant::now() < deadline, "bad peer was not reported");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(before.worker_alive);
    let mut unrelated = document.clone();
    unrelated.set("vfov", "60")?;
    sinks.reconfigure(&document, &unrelated)?;
    let after = sinks.status().zmq.ok_or("publisher lost after edit")?;
    // Recreating the publisher would reset these lifetime counters to zero and
    // disconnect healthy subscribers even though the output settings are equal.
    assert_eq!(after.enqueued, before.enqueued);
    assert_eq!(after.failures, before.failures);
    assert!(after.worker_alive);
    Ok(())
}

#[test]
fn rejected_and_nonfinite_rows_never_reach_either_live_transport() -> TestResult {
    let udp = UdpSocket::bind("127.0.0.1:0")?;
    udp.set_read_timeout(Some(Duration::from_millis(50)))?;
    let document = ConfigDocument::parse(format!(
        "zmq_port:{}\nzmq_output_mode:legacy_matrex_vr\nsock_port:{}\n",
        port()?,
        udp.local_addr()?.port()
    ))?;
    let mut sinks = OutputSinks::from_config(&document)?;
    assert!(sinks.write(&row()).is_empty());
    let mut bytes = [0; 4096];
    udp.recv(&mut bytes)?;
    let before = sinks.status().zmq.ok_or("publisher missing")?.enqueued;
    let mut rejected = row();
    rejected.accepted = false;
    rejected.position[0] = f64::NAN;
    assert!(sinks.write(&rejected).is_empty());
    let mut invalid = row();
    invalid.position[0] = f64::NAN;
    assert!(!sinks.write(&invalid).is_empty());
    invalid = row();
    invalid.heading = f64::INFINITY;
    assert!(!sinks.write(&invalid).is_empty());
    invalid = row();
    invalid.error = f64::NAN;
    assert!(!sinks.write(&invalid).is_empty());
    invalid = row();
    invalid.timestamp_ms = f64::NEG_INFINITY;
    assert!(!sinks.write(&invalid).is_empty());
    assert_eq!(
        sinks.status().zmq.ok_or("publisher missing")?.enqueued,
        before
    );
    assert!(udp.recv(&mut bytes).is_err());
    Ok(())
}

#[test]
fn both_json_framings_keep_udp_native_and_payload_axes_unchanged() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        for framing in [ZmqFraming::Single, ZmqFraming::TopicJson] {
            let zmq_port = port()?;
            let udp = UdpSocket::bind("127.0.0.1:0")?;
            udp.set_read_timeout(Some(Duration::from_millis(100)))?;
            let document = ConfigDocument::parse(format!(
                "zmq_port:{zmq_port}\nzmq_output_mode:legacy_matrex_vr\nzmq_framing:{}\nzmq_topic:sphere-motion\nsock_port:{}\n",
                framing.as_str(), udp.local_addr()?.port()
            ))?;
            let mut sinks = OutputSinks::from_config(&document)?;
            let mut subscriber = zeromq::SubSocket::new();
            subscriber.connect(&format!("tcp://127.0.0.1:{zmq_port}")).await?;
            subscriber.subscribe("").await?;
            let mut native = b"FT, ".to_vec();
            write_dat(&mut native, &row())?;
            let deadline = Instant::now() + Duration::from_secs(2);
            let received = loop {
                assert!(sinks.write(&row()).is_empty());
                let mut udp_bytes = [0; 4096];
                let count = udp.recv(&mut udp_bytes)?;
                assert_eq!(&udp_bytes[..count], native.as_slice());
                if let Ok(message) = tokio::time::timeout(Duration::from_millis(20), subscriber.recv()).await {
                    break message?;
                }
                assert!(Instant::now() < deadline, "JSON subscriber never received a message");
            };
            let payload_index = if framing == ZmqFraming::TopicJson {
                assert_eq!(received.len(), 2);
                assert_eq!(received.get(0).ok_or("topic missing")?.as_ref(), b"sphere-motion");
                1
            } else {
                assert_eq!(received.len(), 1);
                0
            };
            let expected = br#"{"x": -12.5, "y": 42.25, "z": 0.0, "yaw": 17.188733853924695, "pitch": 0.0, "roll": 0.0}"#;
            assert_eq!(received.get(payload_index).ok_or("JSON missing")?.as_ref(), expected);
            let status = serde_json::to_value(sinks.status())?;
            assert_eq!(status["zmq_payload"]["mode"], "legacy_matrex_vr");
            assert_eq!(status["zmq_payload"]["framing"], framing.as_str());
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    Ok(())
}
