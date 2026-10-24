use fictrac_control::{Controller, Request};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
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
