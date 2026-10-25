use fictrac_core::TrackRow;
use fictrac_io::{ConfigDocument, OutputSinks, load_config_text, write_dat};
use std::{
    net::{TcpListener, UdpSocket},
    time::{Duration, Instant},
};
use zeromq::{Socket, SocketRecv};
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn port() -> Result<u16, Box<dyn std::error::Error>> {
    let socket = TcpListener::bind("127.0.0.1:0")?;
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
    let zmq_port = port()?;
    let udp = UdpSocket::bind("127.0.0.1:0")?;
    udp.set_read_timeout(Some(Duration::from_millis(100)))?;
    let document = ConfigDocument::parse(format!(
        "sock_host:127.0.0.1\nsock_port:{}\nzmq_host:localhost\nzmq_port:{zmq_port}\nzmq_queue_capacity:8\n",
        udp.local_addr()?.port()
    ))?;
    let mut sinks = OutputSinks::from_config(&document)?;
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
fn occupied_pub_port_is_actionable_and_disabled_pub_opens_no_socket() -> TestResult {
    let occupied = TcpListener::bind("127.0.0.1:0")?;
    let port = occupied.local_addr()?.port();
    let doc = ConfigDocument::parse(format!("zmq_port:{port}\n"))?;
    let error = match OutputSinks::from_config(&doc) {
        Ok(_) => return Err("occupied port unexpectedly bound".into()),
        Err(error) => error,
    };
    assert!(error.to_string().contains("cannot bind ZMQ PUB"));
    let disabled = ConfigDocument::parse("zmq_port:-1\n".into())?;
    let sinks = OutputSinks::from_config(&disabled)?;
    assert!(!sinks.active());
    assert!(sinks.status().zmq.is_none());
    Ok(())
}
#[test]
fn reconfigure_reuses_healthy_outputs_and_restores_previous_pub_after_busy_port() -> TestResult {
    let old_port = port()?;
    let mut old = ConfigDocument::parse(format!("zmq_port:{old_port}\n"))?;
    let mut sinks = OutputSinks::from_config(&old)?;
    assert!(sinks.write(&row()).is_empty());
    sinks.reconfigure(&old, &old)?;
    assert_eq!(sinks.status().zmq.ok_or("missing publisher")?.enqueued, 1);

    let busy = TcpListener::bind("127.0.0.1:0")?;
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
    assert_eq!(restored.endpoint, format!("tcp://127.0.0.1:{old_port}"));
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
        ("zmq_host", "0.0.0.0"),
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
        assert_eq!(
            document.get("zmq_port"),
            Some((9870 + index).to_string().as_str())
        );
        (document, path) = document.next_instance(path.as_deref())?;
    }
    assert!(path.is_none());
    assert_eq!(document.get("rust_instance_name"), Some("VR5"));
    assert_eq!(document.get("src_fn"), Some("7"));
    assert_eq!(document.get("sock_port"), Some("1115"));
    assert_eq!(document.get("zmq_port"), Some("9875"));
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
