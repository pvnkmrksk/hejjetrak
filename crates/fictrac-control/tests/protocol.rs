use fictrac_control::{Controller, Request};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[test]
fn replacement_exclusions_are_atomic_undoable_and_keep_other_calibration_and_clock() -> Result {
    let mut controller = Controller::new();
    let id = open(&mut controller)?;
    request(&mut controller, "step", Some(id), json!({"frames":2}))?;
    let before = request(&mut controller, "config", Some(id), json!({}))?;
    request(
        &mut controller,
        "calibrate",
        Some(id),
        json!({"kind":"replace_exclusions","polygons":[[[10,10],[25,10],[20,30]]]}),
    )?;
    let edited = request(&mut controller, "config", Some(id), json!({}))?;
    assert_eq!(edited["fields"]["roi_circ"], before["fields"]["roi_circ"]);
    assert_eq!(edited["fields"]["c2a_r"], before["fields"]["c2a_r"]);
    let invalid = controller.execute(Request {
        id: json!("invalid replacement"),
        command: "calibrate".into(),
        session: Some(id),
        args: json!({"kind":"replace_exclusions","polygons":[[[1,1],[2,2],[3,3]]]}),
    });
    assert!(!invalid.ok);
    assert_eq!(
        request(&mut controller, "config", Some(id), json!({}))?,
        edited
    );
    request(&mut controller, "undo", Some(id), json!({}))?;
    assert_eq!(
        request(&mut controller, "config", Some(id), json!({}))?["text"],
        before["text"]
    );
    request(
        &mut controller,
        "calibrate",
        Some(id),
        json!({"kind":"replace_exclusions","polygons":[]}),
    )?;
    let empty = request(&mut controller, "config", Some(id), json!({}))?;
    let document = fictrac_io::ConfigDocument::parse(
        empty["text"]
            .as_str()
            .ok_or("missing config text")?
            .to_owned(),
    )?;
    assert!(document.vector::<Vec<Vec<f64>>>("roi_ignr")?.is_empty());
    let next = request(&mut controller, "step", Some(id), json!({}))?;
    assert!(
        (next["status"]["last_row"]["timestamp_ms"]
            .as_f64()
            .ok_or("missing source timestamp")?
            - 2000.0 / 30.0)
            .abs()
            < 1e-8
    );
    Ok(())
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "fictrac-control-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn request(c: &mut Controller, command: &str, session: Option<u64>, args: Value) -> Result<Value> {
    let response = c.execute(Request {
        id: json!(command),
        command: command.into(),
        session,
        args,
    });
    assert_eq!(response.id, command);
    assert!(response.ok, "{:?}", response.error);
    Ok(response.result.ok_or("missing result")?)
}
fn open(c: &mut Controller) -> Result<u64> {
    let value = request(
        c,
        "open",
        None,
        json!({"config":root().join("examples/benchmark/config.txt"),"input":root().join("examples/sample.mp4")}),
    )?;
    Ok(value["session"].as_u64().ok_or("missing session")?)
}
#[test]
fn complete_video_agent_transcript_matches_declared_reference_gates() -> Result {
    let temp = Temp::new()?;
    let mut c = Controller::new();
    let id = open(&mut c)?;
    let result = request(&mut c, "run", Some(id), json!({"frames":1000}))?;
    assert_eq!(result["processed"], 300);
    assert_eq!(result["status"]["ended"], true);
    assert_eq!(result["status"]["last_row"]["frame"], 299);
    request(
        &mut c,
        "snapshot",
        Some(id),
        json!({"directory":temp.0.join("images")}),
    )?;
    assert!(fs::metadata(temp.0.join("images/accumulated-map.pgm"))?.len() > 1000);
    request(
        &mut c,
        "export",
        Some(id),
        json!({"directory":temp.0.join("export")}),
    )?;
    let comparison = fictrac_io::compare_dat(
        std::io::BufReader::new(fs::File::open(
            root().join("examples/benchmark/upstream.dat"),
        )?),
        std::io::BufReader::new(fs::File::open(temp.0.join("export/tracking.dat"))?),
    )?;
    assert!(comparison.passes(0.075, 0.025, 0.020), "{comparison:?}");
    assert_eq!(
        fs::read_to_string(temp.0.join("export/all-frames.dat"))?
            .lines()
            .count(),
        300
    );
    Ok(())
}
#[test]
fn outside_square_edit_keeps_config_source_and_numeric_results() -> Result {
    let mut controller = Controller::new();
    let edited = open(&mut controller)?;
    let reference = open(&mut controller)?;
    for id in [edited, reference] {
        request(&mut controller, "step", Some(id), json!({"frames":2}))?;
    }
    let before = request(&mut controller, "config", Some(edited), json!({}))?;
    let status = request(&mut controller, "status", Some(edited), json!({}))?;
    let invalid = controller.execute(Request {
        id: json!("outside-square"),
        command: "calibrate".into(),
        session: Some(edited),
        args: json!({"kind":"square","plane":"xy","points":[[-1.,171.],[128.,272.],[20.,212.],[99.,132.]]}),
    });
    assert!(!invalid.ok);
    assert_eq!(
        request(&mut controller, "config", Some(edited), json!({}))?,
        before
    );
    assert_eq!(
        request(&mut controller, "status", Some(edited), json!({}))?,
        status
    );
    let after = request(&mut controller, "step", Some(edited), json!({}))?;
    let expected = request(&mut controller, "step", Some(reference), json!({}))?;
    assert_eq!(after["status"]["total_frames"], 3);
    assert_eq!(after["status"]["last_row"], expected["status"]["last_row"]);
    Ok(())
}
#[test]
fn four_independent_sessions_edit_transactionally_and_calibrate() -> Result {
    let mut c = Controller::new();
    let first = open(&mut c)?;
    let mut ids = vec![first];
    for _ in 0..3 {
        let id = *ids.last().ok_or("missing previous session")?;
        let next = request(&mut c, "new_instance", Some(id), json!({}))?;
        ids.push(next["session"].as_u64().ok_or("missing next ID")?);
    }
    for id in &ids {
        request(&mut c, "step", Some(*id), json!({"frames":2}))?;
    }
    let before = request(&mut c, "config", Some(first), json!({}))?;
    let bad = c.execute(Request {
        id: json!("bad"),
        command: "set".into(),
        session: Some(first),
        args: json!({"key":"vfov","value":"NaN"}),
    });
    assert!(!bad.ok);
    assert_eq!(before, request(&mut c, "config", Some(first), json!({}))?);
    request(
        &mut c,
        "calibrate",
        Some(first),
        json!({"kind":"rim","points":[[63.,171.],[81.,145.],[106.,135.],[150.,160.]]}),
    )?;
    let after = request(&mut c, "config", Some(first), json!({}))?;
    assert!(after["fields"]["roi_c"].is_string());
    assert!(after["fields"]["roi_r"].is_string());
    assert_eq!(
        request(&mut c, "status", Some(ids[1]), json!({}))?["frames"],
        2
    );
    assert_eq!(
        request(&mut c, "status", Some(first), json!({}))?["frames"],
        1
    );
    request(&mut c, "undo", Some(first), json!({}))?;
    assert_eq!(
        request(&mut c, "config", Some(first), json!({}))?["text"],
        before["text"]
    );
    request(&mut c, "redo", Some(first), json!({}))?;
    request(
        &mut c,
        "calibrate",
        Some(first),
        json!({"kind":"exclude","points":[[4.,4.],[10.,4.],[10.,10.]]}),
    )?;
    request(
        &mut c,
        "calibrate",
        Some(first),
        json!({"kind":"remove_exclusion"}),
    )?;
    request(
        &mut c,
        "calibrate",
        Some(first),
        json!({"kind":"forward","start":[100.,100.],"end":[150.,120.]}),
    )?;
    request(
        &mut c,
        "calibrate",
        Some(first),
        json!({"kind":"square","plane":"xy","points":[[191.,171.],[128.,272.],[20.,212.],[99.,132.]]}),
    )?;
    Ok(())
}
#[test]
fn reset_keeps_source_counter_and_recording_exposes_rejections() -> Result {
    let temp = Temp::new()?;
    let mut c = Controller::new();
    let id = open(&mut c)?;
    request(
        &mut c,
        "set",
        Some(id),
        json!({"fields":{"save_raw":"y","save_debug":"y","vid_codec":"mjpg","opt_max_err":"0"}}),
    )?;
    request(
        &mut c,
        "record_start",
        Some(id),
        json!({"directory":temp.0.join("record")}),
    )?;
    request(&mut c, "run", Some(id), json!({"frames":4}))?;
    request(&mut c, "record_stop", Some(id), json!({}))?;
    let all = fs::read_to_string(temp.0.join("record/all-frames.dat"))?;
    let accepted = fs::read_to_string(temp.0.join("record/tracking.dat"))?;
    assert_eq!(all.lines().count(), 4);
    assert!(accepted.lines().count() < 4);
    for video in ["raw.avi", "debug.avi"] {
        let bytes = fs::read(temp.0.join("record").join(video))?;
        assert_eq!(&bytes[..4], b"RIFF");
        assert!(bytes.windows(4).any(|v| v == b"idx1"));
    }
    let before = request(&mut c, "status", Some(id), json!({}))?;
    request(&mut c, "reset", Some(id), json!({}))?;
    let after = request(&mut c, "step", Some(id), json!({}))?;
    assert_eq!(
        after["status"]["last_row"]["frame"].as_u64(),
        before["last_row"]["frame"].as_u64().map(|v| v + 1)
    );
    assert_eq!(after["status"]["last_row"]["sequence"], 0);
    assert!(
        after["status"]["last_row"]["timestamp_ms"].as_f64()
            > before["last_row"]["timestamp_ms"].as_f64()
    );
    let map = temp.0.join("map.png");
    request(&mut c, "map_save", Some(id), json!({"path":map}))?;
    request(&mut c, "map_load", Some(id), json!({"path":map}))?;
    Ok(())
}
#[test]
fn jsonl_errors_recover_without_mutating_other_requests() -> Result {
    let input = format!(
        "{{bad json}}\n{}\n{}\n",
        json!({"id":8,"command":"discover"}),
        json!({"id":9,"command":"status","session":999})
    );
    let mut out = vec![];
    fictrac_control::serve(&mut std::io::Cursor::new(input), &mut out)?;
    let responses: Vec<Value> = String::from_utf8(out)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[0]["error"]["code"], "invalid_request");
    assert_eq!(responses[1]["id"], 8);
    assert_eq!(responses[1]["ok"], true);
    assert_eq!(responses[2]["ok"], false);
    Ok(())
}
#[test]
fn mcp_client_initializes_discovers_and_processes_video() -> Result {
    let config = root().join("examples/benchmark/config.txt");
    let video = root().join("examples/sample.mp4");
    let messages = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test-agent","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"fictrac_execute","arguments":{"command":"open","args":{"config":config,"input":video}}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"fictrac_execute","arguments":{"command":"step","session":1,"args":{"frames":2}}}}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"fictrac_execute","arguments":{"command":"set","session":1,"args":{"key":"vfov","value":"bad"}}}}),
    ];
    let input = messages
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = vec![];
    fictrac_control::serve_mcp(&mut std::io::Cursor::new(input), &mut out)?;
    let responses: Vec<Value> = String::from_utf8(out)?
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(responses.len(), 5);
    assert_eq!(
        responses[0]["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    assert_eq!(
        responses[1]["result"]["tools"][0]["name"],
        "fictrac_execute"
    );
    assert_eq!(
        responses[3]["result"]["structuredContent"]["result"]["processed"],
        2
    );
    assert_eq!(responses[4]["result"]["isError"], true);
    Ok(())
}
#[test]
fn source_fps_override_uses_frame_index_not_host_clock() -> Result {
    let mut c = Controller::new();
    let id = open(&mut c)?;
    request(
        &mut c,
        "set",
        Some(id),
        json!({"key":"src_fps","value":"100"}),
    )?;
    let result = request(&mut c, "run", Some(id), json!({"frames":3}))?;
    assert_eq!(result["status"]["last_row"]["timestamp_ms"], 20.0);
    assert_eq!(result["status"]["last_row"]["delta_ms"], 10.0);
    Ok(())
}

#[test]
fn configuration_edit_preserves_current_video_position_and_fps_counter() -> Result {
    let mut c = Controller::new();
    let id = open(&mut c)?;
    request(&mut c, "run", Some(id), json!({"frames":3}))?;
    request(
        &mut c,
        "set",
        Some(id),
        json!({"key":"src_fps","value":"50"}),
    )?;
    let status = request(&mut c, "status", Some(id), json!({}))?;
    assert!(
        (status["last_row"]["timestamp_ms"]
            .as_f64()
            .ok_or("no clock")?
            - 2000. / 30.)
            .abs()
            < 1e-9
    );
    assert_eq!(status["frames"], 1);
    let after = request(&mut c, "step", Some(id), json!({}))?;
    assert!(
        (after["status"]["last_row"]["timestamp_ms"]
            .as_f64()
            .ok_or("no clock")?
            - (2000. / 30. + 20.))
            .abs()
            < 1e-9
    );
    assert_eq!(after["status"]["total_frames"], 4);
    request(&mut c, "rewind", Some(id), json!({}))?;
    let rewound = request(&mut c, "step", Some(id), json!({}))?;
    assert_eq!(rewound["status"]["last_row"]["timestamp_ms"], 0.0);
    Ok(())
}

#[test]
fn changing_and_disabling_fps_keeps_clock_monotonic() -> Result {
    let mut c = Controller::new();
    let id = open(&mut c)?;
    request(&mut c, "run", Some(id), json!({"frames":3}))?;
    request(
        &mut c,
        "set",
        Some(id),
        json!({"key":"src_fps","value":"60"}),
    )?;
    let first = request(&mut c, "step", Some(id), json!({}))?;
    let t1 = first["status"]["last_row"]["timestamp_ms"]
        .as_f64()
        .ok_or("no clock")?;
    assert!((t1 - (2000. / 30. + 1000. / 60.)).abs() < 1e-9);
    request(
        &mut c,
        "set",
        Some(id),
        json!({"key":"src_fps","value":"-1"}),
    )?;
    let second = request(&mut c, "step", Some(id), json!({}))?;
    let t2 = second["status"]["last_row"]["timestamp_ms"]
        .as_f64()
        .ok_or("no clock")?;
    assert!((t2 - t1 - 1000. / 30.).abs() < 1e-9);
    Ok(())
}
#[test]
fn realtime_is_typed_and_paces_explicit_runs_without_changing_numerics() -> Result {
    let mut c = Controller::new();
    let id = open(&mut c)?;
    let bad = c.execute(Request {
        id: json!("bad-bool"),
        command: "realtime".into(),
        session: Some(id),
        args: json!({"enabled":"true"}),
    });
    assert!(!bad.ok);
    request(&mut c, "realtime", Some(id), json!({"enabled":true}))?;
    let timer = std::time::Instant::now();
    let paced = request(&mut c, "run", Some(id), json!({"frames":3}))?;
    assert!(timer.elapsed().as_secs_f64() >= 2. / 30.);
    assert_eq!(paced["status"]["realtime"], true);
    request(&mut c, "rewind", Some(id), json!({}))?;
    request(&mut c, "realtime", Some(id), json!({"enabled":false}))?;
    let fast = request(&mut c, "run", Some(id), json!({"frames":3}))?;
    assert_eq!(paced["status"]["last_row"], fast["status"]["last_row"]);
    request(&mut c, "close", Some(id), json!({"discard":true}))?;
    Ok(())
}

#[test]
fn instant_record_selectors_preserve_real_video_tracking_and_canonical_config() -> Result {
    let temp = Temp::new()?;
    let mut c = Controller::new();
    let id = open(&mut c)?;
    let reference = open(&mut c)?;
    for session in [id, reference] {
        request(
            &mut c,
            "set",
            Some(session),
            json!({"key":"vid_codec","value":"mjpg"}),
        )?;
        request(&mut c, "run", Some(session), json!({"frames":3}))?;
    }
    let canonical = request(&mut c, "config", Some(id), json!({}))?;
    let epoch = request(&mut c, "status", Some(id), json!({}))?["epoch"].clone();
    for (index, (raw, debug)) in [(true, false), (false, true), (true, true), (false, false)]
        .into_iter()
        .enumerate()
    {
        let directory = temp.0.join(format!("selection-{index}"));
        let status = request(
            &mut c,
            "record_start",
            Some(id),
            json!({"directory":directory,"record_options":{"raw":raw,"debug":debug}}),
        )?;
        assert_eq!(status["record_options"], json!({"raw":raw,"debug":debug}));
        assert_eq!(status["epoch"], epoch);
        request(&mut c, "run", Some(id), json!({"frames":2}))?;
        request(&mut c, "run", Some(reference), json!({"frames":2}))?;
        request(&mut c, "record_stop", Some(id), json!({}))?;
        assert_eq!(directory.join("raw.avi").is_file(), raw);
        assert_eq!(directory.join("debug.avi").is_file(), debug);
        assert_eq!(
            fs::read_to_string(directory.join("all-frames.dat"))?
                .lines()
                .count(),
            2
        );
        let effective =
            fictrac_io::ConfigDocument::parse(fs::read_to_string(directory.join("config.txt"))?)?;
        assert_eq!(effective.boolean("save_raw"), raw);
        assert_eq!(effective.boolean("save_debug"), debug);
        for (name, enabled, dimensions) in [
            ("raw.avi", raw, (384, 288)),
            ("debug.avi", debug, (640, 480)),
        ] {
            if enabled {
                let mut source = fictrac_io::VideoSource::open(&directory.join(name))?;
                let mut count = 0;
                while let Some(frame) = source.next_frame()? {
                    assert_eq!((frame.width(), frame.height()), dimensions);
                    count += 1;
                }
                assert_eq!(count, 2);
            }
        }
        assert_eq!(request(&mut c, "config", Some(id), json!({}))?, canonical);
        let actual = request(&mut c, "status", Some(id), json!({}))?;
        let expected = request(&mut c, "status", Some(reference), json!({}))?;
        assert_eq!(actual["last_row"], expected["last_row"]);
        assert_eq!(actual["total_frames"], expected["total_frames"]);
    }
    let bad = c.execute(Request {
        id: json!("bad-options"),
        command: "record_start".into(),
        session: Some(id),
        args: json!({"directory":temp.0.join("bad"),"record_options":{"raw":"yes","debug":false}}),
    });
    assert!(!bad.ok);
    assert!(!temp.0.join("bad").exists());
    Ok(())
}
#[test]
fn square_cycle_failure_is_atomic_while_real_video_continues() -> Result {
    let mut c = Controller::new();
    let id = open(&mut c)?;
    let offset = 144. / (22.5f64.to_radians().tan()) * 0.1;
    let points = [
        192. + offset,
        144. - offset,
        192. + offset,
        144. + offset,
        192. - offset,
        144. + offset,
        192. - offset,
        144. - offset,
    ];
    request(
        &mut c,
        "set",
        Some(id),
        json!({"fields":{"c2a_r":"{0,0,0}","c2a_t":"{0,0,5}","c2a_src":"c2a_cnrs_xy","c2a_cnrs_xy":serde_json::to_string(&points)?}}),
    )?;
    request(&mut c, "run", Some(id), json!({"frames":3}))?;
    let config = request(&mut c, "config", Some(id), json!({}))?;
    let status = request(&mut c, "status", Some(id), json!({}))?;
    let result = c.execute(Request {
        id: json!("cycle"),
        command: "calibrate".into(),
        session: Some(id),
        args: json!({"kind":"square_cycle"}),
    });
    assert!(!result.ok);
    assert!(
        result
            .error
            .is_some_and(|e| e.message.contains("no distinct accurate square pose"))
    );
    assert_eq!(request(&mut c, "config", Some(id), json!({}))?, config);
    assert_eq!(request(&mut c, "status", Some(id), json!({}))?, status);
    let continued = request(&mut c, "step", Some(id), json!({}))?;
    assert!(
        (continued["status"]["last_row"]["timestamp_ms"]
            .as_f64()
            .ok_or("no timestamp")?
            - 100.)
            .abs()
            < 1e-9
    );
    Ok(())
}
#[test]
fn native_only_controls_are_typed_and_not_advertised_for_headless_sessions() -> Result {
    let schema = fictrac_control::headless_request_schema();
    let variants = schema["oneOf"].as_array().ok_or("missing variants")?;
    assert!(!variants.iter().any(|v| matches!(
        v["properties"]["command"]["const"].as_str(),
        Some("view" | "camera_status" | "camera_set" | "camera_format")
    )));
    let mut c = Controller::new();
    for (command, args) in [
        ("view", json!({"settings":false})),
        ("camera_status", json!({})),
        (
            "camera_set",
            json!({"control":"exposure_mode","mode":"locked"}),
        ),
        ("camera_format", json!({"width":640,"height":480,"fps":30})),
    ] {
        fictrac_control::validate_request_arguments(command, &args)?;
        let response = c.execute(Request {
            id: json!(command),
            command: command.into(),
            session: Some(1),
            args,
        });
        assert!(!response.ok);
        assert!(
            response
                .error
                .is_some_and(|error| error.message.contains("native desktop session"))
        );
    }
    assert!(
        fictrac_control::validate_request_arguments(
            "camera_set",
            &json!({"control":"exposure","value":"10ms"})
        )
        .is_err()
    );
    assert!(
        fictrac_control::validate_request_arguments("view", &json!({"overlays":"false"})).is_err()
    );
    fictrac_control::validate_request_arguments(
        "camera_format",
        &json!({"width":640,"height":480,"fps":null}),
    )?;
    fictrac_control::validate_request_arguments(
        "camera_set",
        &json!({"control":"exposure","value":-6.}),
    )?;
    assert!(
        fictrac_control::validate_request_arguments(
            "camera_format",
            &json!({"width":8192,"height":8192})
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn live_output_actor_survives_parameter_edits_undo_and_video_rewind() -> Result {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let mut c = Controller::new();
    let id = open(&mut c)?;
    request(
        &mut c,
        "set",
        Some(id),
        json!({"fields":{"zmq_host":"127.0.0.1","zmq_port":port.to_string(),"zmq_queue_capacity":"8"}}),
    )?;
    let before = request(&mut c, "run", Some(id), json!({"frames":3}))?;
    let enqueued = before["status"]["outputs"]["zmq"]["enqueued"]
        .as_u64()
        .ok_or("ZMQ status missing")?;
    assert!(enqueued >= 1);
    request(
        &mut c,
        "set",
        Some(id),
        json!({"key":"thr_ratio","value":"1.2"}),
    )?;
    request(&mut c, "undo", Some(id), json!({}))?;
    let after = request(&mut c, "step", Some(id), json!({}))?;
    assert!(
        after["status"]["outputs"]["zmq"]["enqueued"]
            .as_u64()
            .ok_or("lost ZMQ actor")?
            > enqueued
    );
    request(&mut c, "rewind", Some(id), json!({}))?;
    let continued = request(&mut c, "step", Some(id), json!({}))?;
    assert!(
        continued["status"]["outputs"]["zmq"]["enqueued"]
            .as_u64()
            .ok_or("lost ZMQ on rewind")?
            > enqueued
    );
    let document = request(&mut c, "config", Some(id), json!({}))?;
    let status = request(&mut c, "status", Some(id), json!({}))?;
    let invalid = c.execute(Request {
        id: json!("bad-output"),
        command: "set".into(),
        session: Some(id),
        args: json!({"key":"zmq_port","value":"70000"}),
    });
    assert!(!invalid.ok);
    assert_eq!(request(&mut c, "config", Some(id), json!({}))?, document);
    assert_eq!(
        request(&mut c, "status", Some(id), json!({}))?["last_row"],
        status["last_row"]
    );
    let next = request(&mut c, "step", Some(id), json!({}))?;
    assert!(
        (next["status"]["last_row"]["timestamp_ms"]
            .as_f64()
            .ok_or("clock lost after output rollback")?
            - 1000. / 30.)
            .abs()
            < 1e-9
    );
    Ok(())
}

#[test]
fn matrex_recording_processes_the_real_video_with_no_subscriber_or_half_greeting() -> Result {
    use std::io::Write;
    let temp = Temp::new()?;
    let discovery = fictrac_control::schema();
    let defaults = &discovery["capabilities"]["zmq_output"];
    assert_eq!(defaults["default_enabled"], true);
    assert_eq!(defaults["default_host"], "0.0.0.0");
    assert_eq!(defaults["default_port"], 9871);
    assert_eq!(
        discovery["capabilities"]["zmq_output"]["default_mode"],
        "native_ft"
    );
    assert_eq!(
        discovery["capabilities"]["zmq_output"]["legacy_matrex_vr"]["accepted_only"],
        true
    );
    for framing in ["single", "topic_json"] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let mut c = Controller::new();
        let id = open(&mut c)?;
        let configured = request(
            &mut c,
            "set",
            Some(id),
            json!({"fields":{
                "zmq_host":"127.0.0.1", "zmq_port":port.to_string(),
                "zmq_queue_capacity":"1", "zmq_output_mode":"legacy_matrex_vr",
                "zmq_framing":framing, "zmq_topic":"FT"
            }}),
        )?;
        assert_eq!(
            configured["outputs"]["zmq_payload"]["mode"],
            "legacy_matrex_vr"
        );
        assert_eq!(configured["outputs"]["zmq_payload"]["framing"], framing);
        // A TCP client that never completes greeting/subscription cannot become
        // a dependency of numerical recording or starve a complete video run.
        let mut partial = std::net::TcpStream::connect(("127.0.0.1", port))?;
        partial.write_all(&[0xff])?;
        let directory = temp.0.join(framing);
        request(
            &mut c,
            "record_start",
            Some(id),
            json!({
                "directory":directory,"record_options":{"raw":false,"debug":false}
            }),
        )?;
        let output = request(&mut c, "run", Some(id), json!({"frames":1000}))?;
        assert_eq!(output["processed"], 300);
        assert_eq!(output["status"]["ended"], true);
        assert_eq!(output["status"]["recording"], false);
        assert_eq!(output["status"]["last_row"]["frame"], 299);
        let all = fs::read_to_string(directory.join("all-frames.dat"))?;
        let accepted = fs::read_to_string(directory.join("tracking.dat"))?;
        let quality = fs::read_to_string(directory.join("quality.csv"))?;
        assert_eq!(all.lines().count(), 300);
        // The legacy initializer is accepted too; 299 following rows are tracked.
        assert_eq!(accepted.lines().count(), 300);
        assert_eq!(quality.lines().count(), 301);
        let comparison = fictrac_io::compare_dat(
            std::io::BufReader::new(fs::File::open(
                root().join("examples/benchmark/upstream.dat"),
            )?),
            std::io::BufReader::new(fs::File::open(directory.join("tracking.dat"))?),
        )?;
        assert!(
            comparison.passes(0.075, 0.025, 0.020),
            "{framing}: {comparison:?}"
        );
        drop(partial);
    }
    Ok(())
}

#[test]
fn resolution_commands_are_explicit_atomic_undoable_and_preserve_source_clock() -> Result {
    let mut c = Controller::new();
    let id = open(&mut c)?;
    request(&mut c, "step", Some(id), json!({"frames":2}))?;
    request(
        &mut c,
        "calibrate",
        Some(id),
        json!({"kind":"resolution_reference"}),
    )?;
    let reference = request(&mut c, "config", Some(id), json!({}))?;
    assert!(reference["fields"]["rust_normalized_roi_ignr"].is_string());
    let status = request(&mut c, "status", Some(id), json!({}))?;
    let invalid = c.execute(Request {
        id: json!("bad-resolution"),
        command: "calibrate".into(),
        session: Some(id),
        args: json!({"kind":"resolution_apply","width":768,"height":400}),
    });
    assert!(!invalid.ok);
    assert_eq!(request(&mut c, "config", Some(id), json!({}))?, reference);
    assert_eq!(
        request(&mut c, "status", Some(id), json!({}))?["last_row"],
        status["last_row"]
    );
    request(
        &mut c,
        "calibrate",
        Some(id),
        json!({"kind":"resolution_apply","width":192,"height":144}),
    )?;
    let resized = request(&mut c, "config", Some(id), json!({}))?;
    assert_eq!(resized["fields"]["roi_c"], reference["fields"]["roi_c"]);
    assert_eq!(resized["fields"]["roi_r"], reference["fields"]["roi_r"]);
    assert_ne!(
        resized["fields"]["roi_ignr"],
        reference["fields"]["roi_ignr"]
    );
    request(&mut c, "undo", Some(id), json!({}))?;
    assert_eq!(
        request(&mut c, "config", Some(id), json!({}))?["text"],
        reference["text"]
    );
    let next = request(&mut c, "step", Some(id), json!({}))?;
    assert!(
        (next["status"]["last_row"]["timestamp_ms"]
            .as_f64()
            .ok_or("missing source clock")?
            - 2000.0 / 30.0)
            .abs()
            < 1e-8
    );
    Ok(())
}
