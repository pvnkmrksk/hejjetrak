//! Bounded JSONL control of the same sessions rendered in native windows.
//! Worker mutations are explicitly asynchronous: clients poll status to observe completion.
use crate::{Desktop, runtime::Command, session::Session};
use fictrac_control::{CommandError, MAX_REQUEST_BYTES, Request, Response};
use fictrac_io::{ConfigDocument, SourceSpec};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, OpenOptions},
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread,
};
type Result<T> = std::result::Result<T, String>;
const COMMANDS: &[&str] = &[
    "discover",
    "schema",
    "sessions",
    "open",
    "new_instance",
    "close",
    "status",
    "config",
    "set",
    "replace_config",
    "source",
    "save_config",
    "undo",
    "redo",
    "start",
    "pause",
    "step",
    "run",
    "reset",
    "rewind",
    "realtime",
    "calibrate",
    "record_start",
    "view",
    "camera_status",
    "camera_set",
    "camera_format",
    "record_stop",
    "map_load",
    "map_save",
    "snapshot",
];
enum Input {
    Request(Request),
    Error(String),
}
pub struct Bridge {
    input: Receiver<Input>,
    output: SyncSender<Response>,
    pending: VecDeque<Response>,
}
impl Bridge {
    pub fn start(ctx: eframe::egui::Context) -> Result<Self> {
        let (input_tx, input) = mpsc::sync_channel(16);
        let (output, output_rx) = mpsc::sync_channel::<Response>(16);
        thread::Builder::new()
            .name("fictrac-agent-input".into())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut reader = stdin.lock();
                loop {
                    let result = read_request(&mut reader);
                    let (event, terminal) = match result {
                        Ok(Some(request)) => (Input::Request(request), false),
                        Ok(None) => break,
                        Err(message) => {
                            let terminal = message.starts_with("stdin read failed:");
                            (Input::Error(message), terminal)
                        }
                    };
                    if input_tx.send(event).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                    if terminal {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        thread::Builder::new()
            .name("fictrac-agent-output".into())
            .spawn(move || {
                let stdout = std::io::stdout();
                let mut writer = stdout.lock();
                while let Ok(response) = output_rx.recv() {
                    if serde_json::to_writer(&mut writer, &response).is_err()
                        || writeln!(writer).is_err()
                        || writer.flush().is_err()
                    {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            input,
            output,
            pending: VecDeque::new(),
        })
    }
    pub fn poll(&mut self, app: &mut Desktop) {
        while let Some(response) = self.pending.pop_front() {
            match self.output.try_send(response) {
                Ok(()) => {}
                Err(TrySendError::Full(response)) => {
                    self.pending.push_front(response);
                    return;
                }
                Err(TrySendError::Disconnected(_)) => return,
            }
        }
        for _ in 0..8 {
            let Ok(input) = self.input.try_recv() else {
                break;
            };
            let response = match input {
                Input::Request(request) => Self::execute(app, request),
                Input::Error(message) => failure(Value::Null, "invalid_request", message),
            };
            match self.output.try_send(response) {
                Ok(()) => {}
                Err(TrySendError::Full(response)) => {
                    self.pending.push_back(response);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => break,
            }
        }
    }
    pub fn execute(app: &mut Desktop, request: Request) -> Response {
        app.refresh_camera_occupancy();
        let id = request.id.clone();
        match dispatch(app, request) {
            Ok(value) => Response {
                id,
                ok: true,
                result: Some(value),
                error: None,
            },
            Err(message) => failure(id, "command_failed", message),
        }
    }
}
fn failure(id: Value, code: &'static str, message: String) -> Response {
    Response {
        id,
        ok: false,
        result: None,
        error: Some(CommandError { code, message }),
    }
}
fn read_request(reader: &mut impl BufRead) -> Result<Option<Request>> {
    let mut line = vec![];
    let count = reader
        .by_ref()
        .take((MAX_REQUEST_BYTES + 2) as u64)
        .read_until(b'\n', &mut line)
        .map_err(|e| format!("stdin read failed: {e}"))?;
    if count == 0 {
        return Ok(None);
    }
    if line.len() > MAX_REQUEST_BYTES {
        if line.last() != Some(&b'\n') {
            loop {
                let data = reader
                    .fill_buf()
                    .map_err(|e| format!("stdin read failed: {e}"))?;
                if data.is_empty() {
                    break;
                }
                let newline = data.iter().position(|c| *c == b'\n');
                let count = newline.map_or(data.len(), |i| i + 1);
                reader.consume(count);
                if newline.is_some() {
                    break;
                }
            }
        }
        return Err("request exceeds 1 MB; send one bounded JSON object per line".into());
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|e| format!("invalid JSON request: {e}"))
}
fn dispatch(app: &mut Desktop, r: Request) -> Result<Value> {
    fictrac_control::validate_request_arguments(&r.command, &r.args)?;
    match r.command.as_str() {
        "discover" | "schema" => {
            let mut schema = fictrac_control::request_schema();
            if let Some(variants) = schema.get_mut("oneOf").and_then(Value::as_array_mut) {
                variants.retain(|v| {
                    v["properties"]["command"]["const"]
                        .as_str()
                        .is_some_and(|s| COMMANDS.contains(&s))
                });
            }
            return Ok(
                json!({"protocol_version":1,"transport":"native-gui-jsonl","commands":COMMANDS,
                "execution":"GUI-thread config edits are immediate. Worker commands return queued:true. Poll status until frames/revision/paused/recording/notice confirm the requested change; queued is not completion. Each native session closes independently, including the first window. discard:true discards only the selected session's unsaved edits. The application exits after the last session closes.",
                "request_schema":schema,"snapshot":"computed diagnostic images, not an operating-system screenshot"}),
            );
        }
        "sessions" => {
            return Ok(json!(
                app.sessions
                    .iter_mut()
                    .filter(|s| !s.closed)
                    .map(status)
                    .collect::<Vec<_>>()
            ));
        }
        "open" => {
            if app.sessions.iter().filter(|s| !s.closed).count() >= 16 {
                return Err("at most 16 native sessions".into());
            }
            let path = r
                .args
                .get("config")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            let text = if let Some(text) = r.args.get("text").and_then(Value::as_str) {
                text.to_owned()
            } else {
                let path = path
                    .as_ref()
                    .ok_or("open requires args.config or args.text")?;
                if fs::metadata(path).map_err(display)?.len() > MAX_REQUEST_BYTES as u64 {
                    return Err("config exceeds 1 MB".into());
                }
                fs::read_to_string(path).map_err(display)?
            };
            let mut doc = ConfigDocument::parse(text).map_err(display)?;
            if let Some(input) = r.args.get("input").and_then(Value::as_str) {
                doc.set("src_fn", input).map_err(display)?;
            }
            let disk_text = match path.as_ref().filter(|p| p.is_file()) {
                Some(path) => {
                    if fs::metadata(path).map_err(display)?.len() > MAX_REQUEST_BYTES as u64 {
                        return Err("config exceeds 1 MB".into());
                    }
                    Some(fs::read_to_string(path).map_err(display)?)
                }
                None => None,
            };
            preflight_source(&doc, path.as_deref())?;
            let mut session = Session::new(app.next_id, doc, path);
            session.occupied_cameras = app.camera_owners(Some(session.id));
            if let Some(text) = disk_text {
                session.saved = text;
            }
            session.start()?;
            app.next_id = app.next_id.checked_add(1).ok_or("session ID exhausted")?;
            let result = json!({"queued":true,"status":status(&mut session)});
            app.sessions.push(session);
            return Ok(result);
        }
        _ => {}
    }
    let id = r.session.ok_or("this command requires session ID")?;
    let index = app
        .sessions
        .iter()
        .position(|s| s.id == id && !s.closed)
        .ok_or_else(|| format!("unknown native session {id}"))?;
    if r.command == "new_instance" {
        let before = app.sessions.len();
        app.spawn_from(index);
        if app.sessions.len() == before {
            return Err(app.sessions[index].error.clone());
        }
        let session = app
            .sessions
            .last_mut()
            .ok_or("new native session unavailable")?;
        return Ok(json!({"queued":true,"status":status(session)}));
    }
    execute_session(&mut app.sessions[index], &r.command, &r.args)
}
fn status(s: &mut Session) -> Value {
    if let Some(worker) = &s.worker {
        s.snapshot = worker.latest();
    }
    let mut view = json!({"settings":s.show_settings,"diagnostics":s.show_diagnostics,"overlays":s.show_overlays,"full_image":s.full_image,"display_fps":s.display_fps,"calibration_step":match s.tool { crate::session::Tool::Rim => "ball", crate::session::Tool::Exclude => "exclude", crate::session::Tool::Square => "lab", _ => "view" },"edit_marks":s.edit_existing,"magnifier":s.magnifier});
    view["calibration_action"] = json!(if s.calibration_review {
        "inspect"
    } else if s.edit_existing {
        "edit"
    } else if s.calibration_redraw {
        "redraw"
    } else {
        "add"
    });
    view["calibration"] = json!(s.show_settings && s.show_calibration);
    let mut state = json!({"session":s.id,"source":s.source_edit,"config_path":s.path,"native_window":s.native_window,"window_rect":s.window_rect,"window_inner_size":s.window_inner_size,"window_title":s.window_title(),"realtime":s.realtime,
        "frames":s.snapshot.frames,"revision":s.snapshot.revision,"requested_revision":s.revision,
        "paused":s.snapshot.paused,"ended":s.snapshot.ended,"recording":s.snapshot.recording,
        "last_row":s.snapshot.row,"notice":s.snapshot.notice,"error":s.error,"status":s.status,
        "dirty":s.history.text!=s.saved,"disk_conflict":s.disk_conflict,"worker_active":s.worker.is_some(),
        "processing_fps":s.snapshot.processing_fps,"command_error":s.snapshot.command_error,"dropped_running":s.snapshot.dropped_running,"dropped":s.snapshot.dropped,
        "advance_remaining":s.snapshot.advance_remaining,"recording_directory":s.snapshot.recording_directory,
        "record_options":s.snapshot.record_options,"camera":s.snapshot.camera,"outputs":s.snapshot.outputs,
        "processing_ms":s.snapshot.processing_ms,"tracking_ms":s.snapshot.tracking_ms,"tracking_samples":s.snapshot.tracking_samples,"actual_fps":s.snapshot.actual_fps,"source_fps":s.snapshot.source_fps,"headroom_percent":s.snapshot.headroom_percent,
        "view":view,"source_label":s.snapshot.source_label,"resources":s.snapshot.resources});
    state["window_max_inner_size"] = json!([s.max_window_inner_size.x, s.max_window_inner_size.y]);
    state["recording_encoding"] = json!(s.recording_encoding);
    state["media_recording"] = json!(s.snapshot.media_recording);
    state["recording_finalizations"] = json!(s.snapshot.recording_finalizations);
    state["media_warnings"] = json!(s.snapshot.media_warnings);
    state["preview_fps"] = json!(s.snapshot.preview_fps);
    state["preview_target_fps"] = json!(s.snapshot.display_fps);
    let calibration_layers = [
        ("ball", crate::session::Tool::Rim),
        ("exclude", crate::session::Tool::Exclude),
        ("lab", crate::session::Tool::Square),
    ].map(|(step, tool)| json!({"step":step,"saved":s.saved_layer_available(tool),"saved_marks":s.saved_mark_count_for(tool),"summary":s.saved_calibration_summary_for(tool)}));
    state["calibration_summary"] = json!({
        "visual_check_required": true,
        "configuration_valid": s.core_config().is_ok(),
        "source_dimensions": s.snapshot.image.as_ref().map(|image| [image.width, image.height]),
        "layers": calibration_layers
    });
    state
}
fn send(s: &mut Session, command: Command) -> Result<Value> {
    s.worker
        .as_ref()
        .ok_or("session has no running worker; start it first")?
        .send(command)?;
    Ok(json!({"queued":true,"status":status(s)}))
}
fn edited(s: &mut Session, doc: ConfigDocument) -> Result<Value> {
    s.ensure_source_available(&doc)?;
    if doc.get("src_fn") != s.document()?.get("src_fn") {
        preflight_source(&doc, s.path.as_deref().or(s.source_base.as_deref()))?;
    }
    if let Some(image) = &s.snapshot.image {
        fictrac_io::load_config_text(doc.text(), image.width, image.height).map_err(display)?;
    }
    s.edit(doc.text().to_owned());
    s.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
    s.apply()
        .map_err(|e| format!("configuration edited but apply is pending: {e}"))?;
    Ok(json!({"queued":s.worker.is_some(),"status":status(s)}))
}
fn execute_session(s: &mut Session, command: &str, a: &Value) -> Result<Value> {
    match command {
        "status" => Ok(status(s)),
        "config" => {
            let doc = s.document()?;
            Ok(
                json!({"text":doc.text(),"fields":doc.fields().collect::<BTreeMap<_,_>>(),"status":status(s)}),
            )
        }
        "close" => {
            if s.history.text != s.saved
                && !a.get("discard").and_then(Value::as_bool).unwrap_or(false)
            {
                return Err("unsaved native config edits; save_config before closing".into());
            }
            s.worker = None;
            s.closed = true;
            Ok(json!({"queued":true,"closed":s.id}))
        }
        "calibrate" if a.get("kind").and_then(Value::as_str) == Some("square_cycle") => {
            if let Some(worker) = &s.worker {
                s.snapshot = worker.latest();
            }
            let image = s
                .snapshot
                .image
                .as_ref()
                .ok_or("Square cycle needs a current source image")?;
            let (doc, calibration) =
                fictrac_control::cycle_square_document(&s.document()?, image.width, image.height)?;
            let mut result = edited(s, doc)?;
            result["calibration"] = serde_json::to_value(calibration).map_err(display)?;
            Ok(result)
        }
        "source"
            if a.get("input")
                .and_then(Value::as_str)
                .is_some_and(|source| source.parse::<u32>().is_ok()) =>
        {
            let id = string(a, "input")?.parse::<u32>().map_err(display)?;
            s.select_camera(id)?;
            Ok(json!({"queued":true,"status":status(s)}))
        }
        "set" | "source" | "replace_config" | "calibrate" => {
            let mut doc = s.document()?;
            match command {
                "set" => {
                    if let Some(fields) = a.get("fields").and_then(Value::as_object) {
                        if fields.is_empty() {
                            return Err("args.fields must contain at least one field".into());
                        }
                        for (key, value) in fields {
                            doc.set(
                                key,
                                value
                                    .as_str()
                                    .ok_or("field values must be legacy strings")?,
                            )
                            .map_err(display)?;
                        }
                    } else {
                        doc.set(string(a, "key")?, string(a, "value")?)
                            .map_err(display)?;
                    }
                }
                "source" => {
                    doc.set("src_fn", string(a, "input")?).map_err(display)?;
                }
                "replace_config" => {
                    doc = ConfigDocument::parse(string(a, "text")?.to_owned()).map_err(display)?;
                }
                "calibrate" => {
                    calibrate(s, &mut doc, a)?;
                }
                _ => {}
            }
            edited(s, doc)
        }
        "undo" | "redo" => {
            let changed = if command == "undo" {
                s.history.undo()
            } else {
                s.history.redo()
            };
            if changed {
                let doc = s.document()?;
                s.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
                s.changed = Some(std::time::Instant::now());
                s.apply()?;
            }
            Ok(json!({"changed":changed,"queued":changed && s.worker.is_some(),"status":status(s)}))
        }
        "save_config" => save_config(s, Path::new(string(a, "path")?)),
        "start" => {
            if s.worker.is_none() || s.snapshot.ended {
                s.start()?;
            }
            s.paused = false;
            send(s, Command::Pause(false))
        }
        "pause" => {
            s.paused = true;
            send(s, Command::Pause(true))
        }
        "realtime" => {
            let enabled = a
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or("args.enabled must be boolean")?;
            let mut result = send(s, Command::Realtime(enabled))?;
            s.realtime = enabled;
            result["requested_realtime"] = json!(enabled);
            Ok(result)
        }
        "step" | "run" => {
            let frames = a
                .get("frames")
                .and_then(Value::as_u64)
                .unwrap_or(if command == "step" { 1 } else { 10_000 });
            if !(1..=10_000).contains(&frames) {
                return Err("frames must be 1..10000".into());
            }
            s.paused = true;
            let before = s
                .worker
                .as_ref()
                .ok_or("session has no running worker; start it first")?
                .latest()
                .frames;
            let mut result = send(s, Command::Advance(frames as usize))?;
            result["requested_frames"] = json!(frames);
            result["frames_before"] = json!(before);
            result["minimum_frame_target"] = json!(before.saturating_add(frames));
            result["completion"] = json!(
                "Poll status until advance_remaining == 0 and frames >= minimum_frame_target, or ended/error; issue frame-advance commands serially."
            );
            Ok(result)
        }
        "reset" => {
            s.texture_frame = u64::MAX;
            send(s, Command::Reset)
        }
        "rewind" => {
            s.start()?;
            s.paused = true;
            send(s, Command::Pause(true))
        }
        "record_start" => {
            let path = PathBuf::from(string(a, "directory")?);
            if path.exists() {
                return Err("recording directory must be new".into());
            }
            let tuning = fictrac_control::recording_tuning(a)?;
            if let Some(options) = fictrac_control::record_options(a)? {
                let mut result = send(s, Command::RecordOptions(path, options, tuning))?;
                s.record_raw = options.raw;
                s.record_debug = options.debug;
                if a.get("recording_tuning").is_some() {
                    s.recording_encoding = tuning.encoding;
                    result["status"] = status(s);
                }
                result["requested_record_options"] = json!(options);
                result["requested_recording_tuning"] = json!({"encoding": tuning.encoding});
                Ok(result)
            } else if a.get("recording_tuning").is_some() {
                let options = fictrac_control::RecordOptions::from_document(&s.document()?);
                let mut result = send(s, Command::RecordOptions(path, options, tuning))?;
                s.record_raw = options.raw;
                s.record_debug = options.debug;
                s.recording_encoding = tuning.encoding;
                result["status"] = status(s);
                result["requested_recording_tuning"] = json!({"encoding": tuning.encoding});
                Ok(result)
            } else {
                send(s, Command::Record(path))
            }
        }
        "record_stop" => send(s, Command::StopRecord),
        "map_load" => {
            let path = PathBuf::from(string(a, "path")?);
            if !path.is_file() {
                return Err("sphere-map input file does not exist".into());
            }
            // Decoding and quality validation run on the worker; queued is not success.
            s.texture_frame = u64::MAX;
            send(s, Command::LoadMap(path))
        }
        "map_save" => {
            let path = PathBuf::from(string(a, "path")?);
            if path.exists() {
                return Err("sphere-map output must be new".into());
            }
            send(s, Command::SaveMap(path))
        }
        "view" => {
            if a.get("settings").and_then(Value::as_bool) == Some(false)
                && a.get("calibration").and_then(Value::as_bool) == Some(true)
            {
                return Err("calibration:true opens its inspector; omit settings:false".into());
            }
            let display_fps = a
                .get("display_fps")
                .map(|value| {
                    value
                        .as_u64()
                        .filter(|fps| (1..=60).contains(fps))
                        .map(|fps| fps as u32)
                        .ok_or("Display FPS must be an integer between 1 and 60")
                })
                .transpose()?;
            let window_size = a
                .get("window_size")
                .map(|value| -> Result<eframe::egui::Vec2> {
                    let [width, height]: [f32; 2] =
                        serde_json::from_value(value.clone()).map_err(display)?;
                    if !(96.0..=8192.0).contains(&width) || !(96.0..=8192.0).contains(&height) {
                        return Err(
                            "window_size must be [width,height] within 96..8192 logical pixels"
                                .into(),
                        );
                    }
                    if width > s.max_window_inner_size.x || height > s.max_window_inner_size.y {
                        return Err(format!("window_size exceeds this window's current framebuffer limit: {}×{} logical pixels at the current display scale", s.max_window_inner_size.x, s.max_window_inner_size.y));
                    }
                    Ok(eframe::egui::vec2(width, height))
                })
                .transpose()?;
            let proposed_tool = a
                .get("calibration_step")
                .and_then(Value::as_str)
                .map(|step| match step {
                    "ball" => crate::session::Tool::Rim,
                    "exclude" => crate::session::Tool::Exclude,
                    "lab" => crate::session::Tool::Square,
                    _ => crate::session::Tool::Inspect,
                })
                .unwrap_or(s.tool);
            if proposed_tool != s.tool && !s.points.is_empty() {
                return Err(
                    "Apply or cancel the current draft before changing calibration steps".into(),
                );
            }
            if a.get("edit_marks").and_then(Value::as_bool) == Some(true) {
                s.validate_calibration_action(proposed_tool, "edit")?;
            }
            if let Some(action) = a.get("calibration_action").and_then(Value::as_str) {
                s.validate_calibration_action(proposed_tool, action)?;
            }
            if let Some(fps) = display_fps {
                s.set_display_fps(fps)?;
            }
            if a.get("calibration").and_then(Value::as_bool) == Some(true) {
                s.set_calibration_panel(true);
            }
            if proposed_tool != s.tool {
                s.tool = proposed_tool;
                s.begin_calibration_step();
            }
            if let Some(value) = a.get("edit_marks").and_then(Value::as_bool) {
                s.edit_existing = value;
                s.calibration_review = false;
                s.calibration_redraw = false;
            }
            if let Some(action) = a.get("calibration_action").and_then(Value::as_str) {
                s.choose_calibration_action(action)?;
            }
            if let Some(value) = a.get("magnifier").and_then(Value::as_bool) {
                s.magnifier = value;
            }
            if let Some(value) = a.get("settings").and_then(Value::as_bool)
                && a.get("calibration").and_then(Value::as_bool) != Some(true)
            {
                s.set_settings_panel(value);
            }
            if a.get("calibration").and_then(Value::as_bool) == Some(false) {
                s.set_calibration_panel(false);
            }
            if let Some(value) = a.get("diagnostics").and_then(Value::as_bool) {
                s.show_diagnostics = value;
            }
            if let Some(value) = a.get("overlays").and_then(Value::as_bool) {
                s.show_overlays = value;
            }
            if let Some(value) = a.get("full_image").and_then(Value::as_bool) {
                s.full_image = value;
            }
            if let Some(size) = window_size {
                s.requested_window_size = Some(size);
            }
            Ok(status(s))
        }
        "camera_status" | "camera_set" | "camera_format" => {
            if !matches!(
                s.document()?
                    .source(s.path.as_deref().or(s.source_base.as_deref()))
                    .map_err(display)?,
                SourceSpec::Camera(_)
            ) {
                return Err("Video inputs have no live camera controls; connect a numeric camera source first".into());
            }
            match command {
                "camera_status" => send(s, Command::CameraRefresh),
                "camera_set" => {
                    let request = serde_json::from_value(a.clone()).map_err(display)?;
                    send(s, Command::CameraControl(request))
                }
                _ => {
                    let request = serde_json::from_value(a.clone()).map_err(display)?;
                    s.camera_format(request)?;
                    Ok(json!({"queued":true,"status":status(s)}))
                }
            }
        }
        "snapshot" => snapshot(s, Path::new(string(a, "directory")?)),
        _ => Err(format!(
            "command {command} is not available for native GUI sessions; use discover"
        )),
    }
}
fn preflight_source(doc: &ConfigDocument, anchor: Option<&Path>) -> Result<()> {
    if let SourceSpec::Video(path) = doc.source(anchor).map_err(display)? {
        fictrac_io::VideoSource::open(&path)
            .map_err(|e| format!("source was not changed: {}: {e}", path.display()))?;
    }
    Ok(())
}
fn save_config(s: &mut Session, path: &Path) -> Result<Value> {
    let mut doc = s.document()?;
    if s.path.as_deref() != Some(path)
        && let Ok(SourceSpec::Video(input)) =
            doc.source(s.path.as_deref().or(s.source_base.as_deref()))
        && let Ok(input) = fs::canonicalize(input)
    {
        doc.set("src_fn", &input.to_string_lossy())
            .map_err(display)?;
    }
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(display)?;
    out.write_all(doc.text().as_bytes())
        .and_then(|_| out.sync_all())
        .map_err(display)?;
    s.history.replace(doc.text().to_owned());
    s.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
    s.saved = doc.text().to_owned();
    s.path = Some(path.to_path_buf());
    s.source_base = s.path.clone();
    s.disk_conflict = false;
    Ok(json!({"path":path,"bytes":doc.text().len(),"status":status(s)}))
}
fn calibrate(s: &Session, doc: &mut ConfigDocument, a: &Value) -> Result<()> {
    let image = s
        .snapshot
        .image
        .as_ref()
        .ok_or("no image dimensions available; start the source before calibration")?;
    let config = fictrac_core::Config {
        width: image.width,
        height: image.height,
        vfov_deg: doc.number("vfov", 45.0),
        fisheye: doc.boolean("fisheye"),
        ..fictrac_core::Config::default()
    };
    match string(a, "kind")? {
        "resolution_reference" | "resolution_apply" => {
            let width = a
                .get("width")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(image.width);
            let height = a
                .get("height")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(image.height);
            *doc = if string(a, "kind")? == "resolution_reference" {
                fictrac_io::capture_calibration_reference(doc, width, height)
            } else {
                fictrac_io::rescale_calibration_reference(doc, width, height)
            }
            .map_err(display)?;
        }
        "rim" => {
            let p = points(a)?;
            let sphere = fictrac_core::calibration::fit_rim(&config, &p).map_err(display)?;
            doc.set("roi_c", &legacy(sphere.centre)?).map_err(display)?;
            doc.set("roi_r", &sphere.angular_radius.to_string())
                .map_err(display)?;
            doc.set(
                "roi_circ",
                &legacy(p.into_iter().flatten().collect::<Vec<_>>())?,
            )
            .map_err(display)?;
        }
        "exclude" => {
            let p = points(a)?;
            if !(3..=256).contains(&p.len()) || p.iter().flatten().any(|v| !v.is_finite()) {
                return Err("exclude needs 3..256 finite points".into());
            }
            let area: f64 = p
                .iter()
                .zip(p.iter().cycle().skip(1))
                .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
                .sum();
            if area.abs() < 1.0 {
                return Err("exclusion must enclose a visible area".into());
            }
            let mut polygons: Vec<Vec<f64>> = if doc.get("roi_ignr").is_some() {
                doc.vector("roi_ignr").map_err(display)?
            } else {
                vec![]
            };
            if polygons.len() >= 64 {
                return Err("at most 64 exclusion polygons".into());
            }
            polygons.push(p.into_iter().flatten().collect());
            doc.set("roi_ignr", &legacy(polygons)?).map_err(display)?;
        }
        "replace_exclusions" => {
            let polygons: Vec<Vec<[f64; 2]>> =
                serde_json::from_value(a.get("polygons").ok_or("args.polygons required")?.clone())
                    .map_err(display)?;
            *doc = fictrac_control::replace_exclusions_document(doc, &polygons)?;
        }
        "remove_exclusion" => {
            let mut polygons: Vec<Vec<f64>> = if doc.get("roi_ignr").is_some() {
                doc.vector("roi_ignr").map_err(display)?
            } else {
                vec![]
            };
            polygons.pop();
            doc.set("roi_ignr", &legacy(polygons)?).map_err(display)?;
        }
        "forward" => {
            let start =
                serde_json::from_value(a.get("start").ok_or("args.start required")?.clone())
                    .map_err(display)?;
            let end = serde_json::from_value(a.get("end").ok_or("args.end required")?.clone())
                .map_err(display)?;
            let effective = fictrac_io::load_config_text(doc.text(), image.width, image.height)
                .map_err(display)?
                .config;
            let r = fictrac_core::calibration::forward_rotation(&effective, start, end)
                .map_err(display)?;
            doc.set("c2a_r", &legacy(r)?).map_err(display)?;
        }
        "square" => {
            let p = points(a)?;
            let plane = string(a, "plane")?;
            let (r, t) = fictrac_core::calibration::lab_from_square(
                config.width,
                config.height,
                config.vfov_deg,
                config.fisheye,
                plane,
                &p,
            )
            .map_err(display)?;
            let key = format!("c2a_cnrs_{plane}");
            doc.set("c2a_r", &legacy(r)?).map_err(display)?;
            doc.set("c2a_t", &legacy(t)?).map_err(display)?;
            doc.set(&key, &legacy(p.into_iter().flatten().collect::<Vec<_>>())?)
                .map_err(display)?;
            doc.set("c2a_src", &key).map_err(display)?;
        }
        _ => {
            return Err(
                "calibration kind must be rim/exclude/remove_exclusion/forward/square".into(),
            );
        }
    }
    if !matches!(
        string(a, "kind")?,
        "resolution_reference" | "resolution_apply"
    ) && doc.get("rust_calibration_size").is_some()
    {
        let [width, height]: [usize; 2] = doc.vector("rust_calibration_size").map_err(display)?;
        *doc = fictrac_io::capture_calibration_reference(doc, width, height).map_err(display)?;
    }
    Ok(())
}
fn snapshot(s: &mut Session, directory: &Path) -> Result<Value> {
    if let Some(worker) = &s.worker {
        s.snapshot = worker.latest();
    }
    if s.worker.is_some() && s.snapshot.revision != s.revision {
        return Err("configuration apply is pending; poll status until revision matches requested_revision before taking a snapshot".into());
    }
    let image = s
        .snapshot
        .image
        .as_ref()
        .ok_or("no source image available yet")?;
    let debug = image
        .debug
        .as_ref()
        .ok_or("no numerical diagnostic images available yet")?;
    let doc = s.document()?;
    let config = fictrac_io::load_config_text(doc.text(), image.width, image.height)
        .map_err(display)?
        .config;
    let mosaic = fictrac_io::render_calibrated_debug_rgb(
        &image.rgb,
        &config,
        debug,
        &s.snapshot.path,
        &s.snapshot.pose_history,
        192,
    )
    .map_err(display)?;
    fs::create_dir(directory).map_err(display)?;
    png(
        &directory.join("input.png"),
        image.width,
        image.height,
        &image.rgb,
        image::ExtendedColorType::Rgb8,
    )?;
    for (name, pixels, w, h) in [
        ("roi", &debug.roi, debug.roi_size, debug.roi_size),
        ("warped-diff", &debug.diff, debug.roi_size, debug.roi_size),
        (
            "instant-map",
            &debug.instant,
            debug.map_width,
            debug.map_height,
        ),
        (
            "accumulated-map",
            &debug.accumulated,
            debug.map_width,
            debug.map_height,
        ),
    ] {
        png(
            &directory.join(format!("{name}.png")),
            w,
            h,
            pixels,
            image::ExtendedColorType::L8,
        )?;
    }
    png(
        &directory.join("debug.png"),
        768,
        576,
        &mosaic,
        image::ExtendedColorType::Rgb8,
    )?;
    let state = status(s);
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("status.json"))
        .map_err(display)?;
    serde_json::to_writer_pretty(&mut out, &state).map_err(display)?;
    Ok(
        json!({"directory":directory,"kind":"computed-debug-images","status":state,"files":["input.png","roi.png","warped-diff.png","instant-map.png","accumulated-map.png","debug.png","status.json"]}),
    )
}
fn png(
    path: &Path,
    width: usize,
    height: usize,
    pixels: &[u8],
    color: image::ExtendedColorType,
) -> Result<()> {
    use image::ImageEncoder;
    let out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(display)?;
    image::codecs::png::PngEncoder::new(out)
        .write_image(pixels, width as u32, height as u32, color)
        .map_err(display)
}
fn string<'a>(a: &'a Value, key: &str) -> Result<&'a str> {
    a.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("args.{key} must be a nonempty string"))
}
fn points(a: &Value) -> Result<Vec<[f64; 2]>> {
    serde_json::from_value(a.get("points").ok_or("args.points required")?.clone()).map_err(display)
}
fn legacy(value: impl Serialize) -> Result<String> {
    Ok(serde_json::to_string(&value)
        .map_err(display)?
        .replace('[', "{ ")
        .replace(']', " }"))
}
fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::Cursor,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    };
    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    fn temp() -> Result<PathBuf> {
        let p = std::env::temp_dir().join(format!(
            "fictrac-native-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).map_err(display)?;
        Ok(p)
    }
    fn session() -> Result<Session> {
        let doc =
            ConfigDocument::parse(include_str!("../../../examples/upstream-config.txt").to_owned())
                .map_err(display)?;
        Ok(Session::new(1, doc, None))
    }
    #[test]
    fn bounded_reader_recovers_after_malformed_and_oversized_requests() -> TestResult {
        let mut data = b"not json\n".to_vec();
        data.extend(vec![b'x'; MAX_REQUEST_BYTES + 100]);
        data.extend(b"\n{\"id\":7,\"command\":\"sessions\"}\n");
        let mut input = Cursor::new(data);
        assert!(read_request(&mut input).is_err());
        assert!(read_request(&mut input).is_err());
        let good = read_request(&mut input)?.ok_or("missing recovered request")?;
        assert_eq!(good.id, json!(7));
        assert_eq!(good.command, "sessions");
        assert!(read_request(&mut input)?.is_none());
        Ok(())
    }
    #[test]
    fn agent_edits_share_gui_history_and_save_without_overwrite() -> TestResult {
        let mut s = session()?;
        let original = s.history.text.clone();
        let result = execute_session(&mut s, "set", &json!({"key":"thr_ratio","value":"2.5"}))?;
        assert_eq!(result["queued"], false);
        assert_eq!(s.document()?.get("thr_ratio"), Some("2.5"));
        execute_session(&mut s, "undo", &json!({}))?;
        assert_eq!(s.history.text, original);
        execute_session(&mut s, "redo", &json!({}))?;
        assert_eq!(s.document()?.get("thr_ratio"), Some("2.5"));
        assert!(execute_session(&mut s, "close", &json!({})).is_err());
        let dir = temp()?;
        let path = dir.join("config.txt");
        execute_session(&mut s, "save_config", &json!({"path":path}))?;
        assert_eq!(fs::read_to_string(&path)?, s.history.text);
        assert_eq!(s.history.text, s.saved);
        assert!(execute_session(&mut s, "save_config", &json!({"path":path})).is_err());
        execute_session(&mut s, "close", &json!({}))?;
        assert!(s.closed);
        fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn agent_snapshot_contains_actual_numerical_views() -> TestResult {
        let mut s = Session::new(1,ConfigDocument::parse("vfov : 45\nroi_c : { 0,0,1 }\nroi_r : 0.29\nc2a_r : { 0,0,0 }\nsrc_fn : examples/sample.mp4\n".into())?,None);
        let config = fictrac_core::Config::default();
        let frame = fictrac_core::synthetic::render(&config, [0.0; 3], 0.0)?;
        let mut tracker = fictrac_core::Tracker::new(config.clone())?;
        let row = tracker.process(frame.clone())?;
        let debug =
            fictrac_core::Diagnostics::new(config, 6, 1.25, 0.25)?.update(&frame, Some(&row));
        s.snapshot.row = Some(row);
        s.snapshot.frames = 1;
        s.snapshot.image = Some(Arc::new(crate::runtime::ViewFrame {
            width: frame.width(),
            height: frame.height(),
            rgb: fictrac_control::gray_rgb(&frame),
            debug: Some(debug.clone()),
        }));
        let dir = temp()?;
        let output = dir.join("snapshot");
        let result = execute_session(&mut s, "snapshot", &json!({"directory":output}))?;
        assert_eq!(result["kind"], "computed-debug-images");
        assert_eq!(
            image::open(output.join("roi.png"))?.to_luma8().as_raw(),
            &debug.roi
        );
        assert_eq!(image::open(output.join("debug.png"))?.width(), 768);
        assert!(execute_session(&mut s, "snapshot", &json!({"directory":output})).is_err());
        fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn occupied_camera_selection_is_atomic_for_native_source_commands() -> TestResult {
        let mut s = session()?;
        s.occupied_cameras.insert(2, "VR2 (window 2)".into());
        let text = s.history.text.clone();
        let error = execute_session(&mut s, "source", &json!({"input":"2"}))
            .err()
            .ok_or("occupied camera was accepted")?;
        assert!(error.contains("already used by VR2"));
        assert_eq!(s.history.text, text);
        assert_eq!(s.revision, 0);
        assert!(s.worker.is_none());
        Ok(())
    }
    #[test]
    fn view_resize_is_bounded_and_does_not_mutate_tracking_config() -> TestResult {
        let mut s = session()?;
        let text = s.history.text.clone();
        execute_session(
            &mut s,
            "view",
            &json!({"window_size":[96,96],"diagnostics":true}),
        )?;
        assert_eq!(
            s.requested_window_size,
            Some(eframe::egui::vec2(96.0, 96.0))
        );
        assert!(s.show_diagnostics);
        for size in [
            json!([0, 96]),
            json!([96, 8193]),
            json!([96]),
            json!("96x96"),
        ] {
            assert!(
                execute_session(
                    &mut s,
                    "view",
                    &json!({"window_size":size,"diagnostics":false}),
                )
                .is_err()
            );
            assert!(s.show_diagnostics);
        }
        assert_eq!(s.history.text, text);
        assert_eq!(s.revision, 0);
        Ok(())
    }
    #[test]
    fn first_session_closes_without_discarding_other_unsaved_windows() -> TestResult {
        use clap::Parser;
        let mut child = session()?;
        child.id = 2;
        child.fields(&[("thr_ratio", "2.0".to_owned())])?;
        let mut app = Desktop {
            sessions: vec![session()?, child],
            next_id: 3,
            args: crate::Args::try_parse_from(["fictrac-desktop"])?,
            start: std::time::Instant::now(),
            requested_screenshot: false,
            last_state: std::time::Instant::now(),
            control: None,
            tile_columns: 2,
            window_origin: crate::Pair(12., 36.),
            window_size: crate::Pair(520., 480.),
            root_hidden: false,
        };
        let request =
            serde_json::from_value(json!({"id":"root-close","command":"close","session":1}))?;
        let response = Bridge::execute(&mut app, request);
        assert!(response.ok);
        assert_eq!(response.id, json!("root-close"));
        assert!(app.sessions[0].closed);
        assert!(!app.sessions[1].closed);
        let discover = serde_json::from_value(json!({"id":42,"command":"discover"}))?;
        let response = Bridge::execute(&mut app, discover);
        assert!(response.ok);
        assert!(
            response.result.ok_or("discovery missing")?["commands"]
                .as_array()
                .ok_or("commands missing")?
                .iter()
                .any(|v| v == "calibrate")
        );
        let history = app.sessions[1].history.text.clone();
        let previous = app.sessions[1].show_settings;
        let invalid_view = serde_json::from_value(
            json!({"id":"bad-view","command":"view","session":2,"args":{"settings":!previous,"overlays":"false"}}),
        )?;
        assert!(!Bridge::execute(&mut app, invalid_view).ok);
        assert_eq!(app.sessions[1].show_settings, previous);
        assert_eq!(app.sessions[1].history.text, history);
        Ok(())
    }
    #[test]
    fn agent_rim_calibration_can_repair_missing_geometry_without_a_live_camera() -> TestResult {
        let mut s = Session::new(
            1,
            ConfigDocument::parse(
                "vfov : 45\nsrc_fn : examples/sample.mp4\nc2a_r : { 0,0,0 }\n".into(),
            )?,
            None,
        );
        s.snapshot.image = Some(Arc::new(crate::runtime::ViewFrame {
            width: 160,
            height: 160,
            rgb: vec![0; 160 * 160 * 3],
            debug: None,
        }));
        let p: Vec<_> = (0..16)
            .map(|i| {
                let theta = i as f64 * std::f64::consts::TAU / 16.0;
                [80.0 + 40.0 * theta.cos(), 80.0 + 40.0 * theta.sin()]
            })
            .collect();
        execute_session(&mut s, "calibrate", &json!({"kind":"rim","points":p}))?;
        let doc = s.document()?;
        assert!(doc.get("roi_c").is_some());
        assert!(doc.get("roi_r").is_some());
        assert!(doc.get("roi_circ").is_some());
        let parsed = fictrac_io::load_config_text(doc.text(), 160, 160)?;
        assert!((parsed.config.sphere.angular_radius - 0.204).abs() < 0.01);
        let before = s.history.text.clone();
        assert!(
            execute_session(
                &mut s,
                "calibrate",
                &json!({"kind":"exclude","points":[[1,1],[2,2],[3,3]]})
            )
            .is_err()
        );
        assert_eq!(s.history.text, before);
        Ok(())
    }
    #[test]
    fn invalid_agent_numeric_edits_leave_document_and_revision_unchanged() -> TestResult {
        let mut s = session()?;
        s.snapshot.image = Some(Arc::new(crate::runtime::ViewFrame {
            width: 384,
            height: 288,
            rgb: vec![],
            debug: None,
        }));
        let text = s.history.text.clone();
        let revision = s.revision;
        let value = execute_session(&mut s, "set", &json!({"key":"vfov","value":"NaN"}));
        assert!(value.is_err());
        assert_eq!(s.history.text, text);
        assert_eq!(s.revision, revision);
        assert!(s.changed.is_none());
        Ok(())
    }
    #[test]
    fn invalid_source_does_not_replace_active_configuration() -> TestResult {
        let mut s = session()?;
        let before = s.history.text.clone();
        let revision = s.revision;
        let result = execute_session(
            &mut s,
            "source",
            &json!({"input":"/nonexistent/fictrac/no-video.mp4"}),
        );
        assert!(result.is_err());
        assert_eq!(s.history.text, before);
        assert_eq!(s.revision, revision);
        assert!(s.worker.is_none());
        Ok(())
    }
    #[test]
    fn explicit_discard_allows_closing_unsaved_native_session() -> TestResult {
        let mut s = session()?;
        s.fields(&[("thr_ratio", "2.0".to_owned())])?;
        assert!(execute_session(&mut s, "close", &json!({})).is_err());
        execute_session(&mut s, "close", &json!({"discard":true}))?;
        assert!(s.closed);
        Ok(())
    }
    #[test]
    fn view_controls_and_live_camera_errors_are_explicit_without_document_edits() -> TestResult {
        let mut s = session()?;
        let before = s.history.text.clone();
        let result = execute_session(
            &mut s,
            "view",
            &json!({"settings":true,"diagnostics":true,"overlays":false,"full_image":true}),
        )?;
        assert_eq!(result["view"]["settings"], true);
        assert_eq!(result["view"]["diagnostics"], true);
        assert_eq!(result["view"]["overlays"], false);
        assert_eq!(result["view"]["full_image"], true);
        assert_eq!(s.history.text, before);
        for (command, args) in [
            ("camera_status", json!({})),
            ("camera_set", json!({"control":"gain","value":1.})),
            ("camera_format", json!({"width":640,"height":480,"fps":30.})),
        ] {
            let error = execute_session(&mut s, command, &args)
                .err()
                .ok_or("video camera command unexpectedly succeeded")?;
            assert!(error.contains("Video inputs have no live camera controls"));
            assert_eq!(s.history.text, before);
        }
        Ok(())
    }
    #[test]
    fn native_calibration_choices_review_keep_edit_and_redraw_without_incidental_edits()
    -> TestResult {
        let mut s = session()?;
        let original = s.history.text.clone();
        let review = execute_session(&mut s, "view", &json!({"calibration_step":"ball"}))?;
        assert_eq!(review["view"]["calibration_action"], "inspect");
        let edit = execute_session(&mut s, "view", &json!({"calibration_action":"edit"}))?;
        assert_eq!(edit["view"]["calibration_action"], "edit");
        execute_session(&mut s, "view", &json!({"calibration_action":"inspect"}))?;
        let next = execute_session(&mut s, "view", &json!({"calibration_action":"keep"}))?;
        assert_eq!(next["view"]["calibration_step"], "exclude");
        let draft = execute_session(&mut s, "view", &json!({"calibration_action":"redraw"}))?;
        assert_eq!(draft["view"]["calibration_action"], "redraw");
        assert_eq!(s.history.text, original);
        execute_session(&mut s, "view", &json!({"calibration_action":"inspect"}))?;
        execute_session(&mut s, "view", &json!({"calibration_action":"keep"}))?;
        let done = execute_session(&mut s, "view", &json!({"calibration_action":"keep"}))?;
        assert_eq!(done["view"]["calibration_step"], "view");
        assert_eq!(s.history.text, original);
        Ok(())
    }
    #[test]
    fn daily_calibration_overview_and_skips_preserve_real_video_rows_and_paused_clock() -> TestResult
    {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = ConfigDocument::parse(fs::read_to_string(
            root.join("examples/benchmark/config.txt"),
        )?)?;
        doc.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )?;
        let mut s = Session::new(1, doc, None);
        s.realtime = false;
        s.start()?;
        execute_session(&mut s, "pause", &json!({}))?;
        execute_session(&mut s, "step", &json!({"frames":5}))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let state = status(&mut s);
            if state["frames"].as_u64().unwrap_or(0) >= 5
                && state["advance_remaining"] == 0
                && state["paused"] == true
            {
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err("real-video calibration fixture did not settle".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        s.full_image = false;
        s.show_overlays = false;
        let before = status(&mut s);
        let canonical = s.history.text.clone();
        let opened = execute_session(&mut s, "view", &json!({"calibration":true}))?;
        assert_eq!(opened["view"]["calibration"], true);
        assert_eq!(opened["calibration_summary"]["visual_check_required"], true);
        assert_eq!(
            opened["calibration_summary"]["layers"]
                .as_array()
                .map(Vec::len),
            Some(3)
        );
        execute_session(
            &mut s,
            "view",
            &json!({"calibration_step":"ball","calibration_action":"inspect"}),
        )?;
        for next in ["exclude", "lab", "view"] {
            let state = execute_session(&mut s, "view", &json!({"calibration_action":"keep"}))?;
            assert_eq!(state["view"]["calibration_step"], next);
            assert_eq!(state["view"]["calibration_action"], "inspect");
            assert!(s.paused);
        }
        execute_session(&mut s, "view", &json!({"calibration":false}))?;
        let after = status(&mut s);
        for field in [
            "frames",
            "last_row",
            "revision",
            "requested_revision",
            "paused",
        ] {
            assert_eq!(after[field], before[field], "changed {field}");
        }
        assert!(!s.full_image && !s.show_overlays);
        assert_eq!(s.history.text, canonical);
        execute_session(
            &mut s,
            "view",
            &json!({"calibration":true,"calibration_step":"exclude","calibration_action":"redraw"}),
        )?;
        let invalid_before = status(&mut s);
        assert!(
            execute_session(
                &mut s,
                "view",
                &json!({"calibration_action":"continue","display_fps":15})
            )
            .is_err()
        );
        assert_eq!(status(&mut s)["view"], invalid_before["view"]);
        execute_session(
            &mut s,
            "view",
            &json!({"calibration_action":"cancel","calibration":false}),
        )?;
        assert!(s.paused && s.tool == crate::session::Tool::Inspect);
        assert_eq!(s.history.text, canonical);
        execute_session(&mut s, "step", &json!({"frames":3}))?;
        loop {
            let state = status(&mut s);
            if state["frames"].as_u64().unwrap_or(0) >= before["frames"].as_u64().unwrap_or(0) + 3
                && state["advance_remaining"] == 0
            {
                assert!(
                    state["last_row"]["timestamp_ms"].as_f64()
                        > before["last_row"]["timestamp_ms"].as_f64()
                );
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err("real video did not continue after calibration review".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(())
    }
    #[test]
    fn invalid_calibration_view_choices_preserve_view_document_and_real_source_clock() -> TestResult
    {
        let mut s = session()?;
        s.source_base = Some(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/upstream-config.txt"),
        );
        s.start()?;
        execute_session(&mut s, "pause", &json!({}))?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !status(&mut s)["paused"].as_bool().unwrap_or(false) {
            if std::time::Instant::now() > deadline {
                return Err("worker did not pause".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        execute_session(&mut s, "step", &json!({"frames":2}))?;
        loop {
            let state = status(&mut s);
            if state["frames"].as_u64().unwrap_or(0) >= 2 && state["advance_remaining"] == 0 {
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err("worker did not process sample frames".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        for args in [
            json!({"calibration_step":"ball","calibration_action":"add","settings":true}),
            json!({"calibration_step":"view","calibration_action":"edit","full_image":true}),
        ] {
            let before = status(&mut s);
            let text = s.history.text.clone();
            assert!(execute_session(&mut s, "view", &args).is_err());
            let after = status(&mut s);
            for field in [
                "view",
                "frames",
                "last_row",
                "revision",
                "requested_revision",
                "paused",
            ] {
                assert_eq!(after[field], before[field], "mutated {field}");
            }
            assert_eq!(s.history.text, text);
        }
        let geometry = s.core_config()?;
        let mut doc = s.document()?;
        doc.set("roi_c", &legacy(geometry.sphere.centre)?)?;
        doc.set("roi_r", &geometry.sphere.angular_radius.to_string())?;
        doc.set("roi_circ", "{}")?;
        s.history.replace(doc.text().to_owned());
        let before = status(&mut s);
        let text = s.history.text.clone();
        assert!(
            execute_session(
                &mut s,
                "view",
                &json!({"calibration_step":"ball","calibration_action":"edit","settings":true})
            )
            .is_err()
        );
        let after = status(&mut s);
        assert_eq!(after["view"], before["view"]);
        assert_eq!(after["last_row"], before["last_row"]);
        assert_eq!(s.history.text, text);
        Ok(())
    }
    #[test]
    fn native_record_options_track_real_video_without_config_or_revision_reset() -> TestResult {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = ConfigDocument::parse(fs::read_to_string(
            root.join("examples/benchmark/config.txt"),
        )?)?;
        doc.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )?;
        doc.set("vid_codec", "mjpg")?;
        let mut s = Session::new(1, doc, None);
        s.realtime = false;
        s.start()?;
        s.worker
            .as_ref()
            .ok_or("worker missing")?
            .send(Command::Pause(true))?;
        s.worker
            .as_ref()
            .ok_or("worker missing")?
            .send(Command::Advance(3))?;
        fn wait(
            s: &mut Session,
            predicate: impl Fn(&crate::runtime::Snapshot) -> bool,
        ) -> Result<()> {
            let start = std::time::Instant::now();
            loop {
                s.snapshot = s.worker.as_ref().ok_or("worker gone")?.latest();
                if predicate(&s.snapshot) {
                    return Ok(());
                }
                if s.snapshot.ended {
                    return Err(format!("worker ended: {}", s.snapshot.notice));
                }
                if start.elapsed().as_secs() > 15 {
                    return Err(format!("worker timed out: {}", s.snapshot.notice));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        wait(&mut s, |snapshot| {
            snapshot.frames >= 3 && snapshot.advance_remaining == 0
        })?;
        let before = s.history.text.clone();
        let revision = s.revision;
        let paused_row = serde_json::to_value(&s.snapshot.row)?;
        let paused_frames = s.snapshot.frames;
        for fps in [1, 60] {
            execute_session(&mut s, "view", &json!({"display_fps":fps}))?;
            wait(&mut s, |snapshot| snapshot.display_fps == fps)?;
            assert_eq!(s.snapshot.frames, paused_frames);
            assert_eq!(serde_json::to_value(&s.snapshot.row)?, paused_row);
            assert_eq!(s.history.text, before);
            assert_eq!(s.revision, revision);
        }
        let previous_view = status(&mut s)["view"].clone();
        for invalid in [json!(0), json!(61), json!(1.5), json!("30")] {
            assert!(
                execute_session(
                    &mut s,
                    "view",
                    &json!({"display_fps":invalid,"diagnostics":true})
                )
                .is_err()
            );
            assert_eq!(status(&mut s)["view"], previous_view);
        }
        let directory = temp()?.join("record");
        let record = execute_session(
            &mut s,
            "record_start",
            &json!({"directory":directory,"record_options":{"raw":true,"debug":false},"recording_tuning":{"encoding":"raw"}}),
        )?;
        assert_eq!(record["requested_record_options"]["raw"], true);
        wait(&mut s, |snapshot| {
            snapshot.recording
                && snapshot.record_options
                    == Some(fictrac_control::RecordOptions {
                        raw: true,
                        debug: false,
                    })
        })?;
        let frames = s.snapshot.frames;
        execute_session(&mut s, "step", &json!({"frames":2}))?;
        wait(&mut s, |snapshot| {
            snapshot.frames >= frames + 2 && snapshot.advance_remaining == 0
        })?;
        execute_session(&mut s, "record_stop", &json!({}))?;
        wait(&mut s, |snapshot| !snapshot.recording)?;
        assert_eq!(s.history.text, before);
        assert_eq!(s.revision, revision);
        assert!(fs::read_to_string(directory.join("epoch-0000-config.txt"))?.contains("vid_codec"));
        let effective =
            ConfigDocument::parse(fs::read_to_string(directory.join("epoch-0000-config.txt"))?)?;
        assert_eq!(effective.get("vid_codec"), Some("raw"));
        let media_status = s
            .snapshot
            .media_recording
            .as_ref()
            .ok_or("media status missing")?;
        assert_eq!(media_status.raw.encoded_frames, 2);
        let media = directory.join("epoch-0000-media/raw.avi");
        assert!(media.is_file());
        let mut source = fictrac_io::VideoSource::open(&media)?;
        let mut count = 0;
        while let Some(frame) = source.next_frame()? {
            assert_eq!((frame.width(), frame.height()), (384, 288));
            count += 1;
        }
        assert_eq!(count, 2);
        let last = s.snapshot.row.as_ref().ok_or("last tracking row missing")?;
        assert!(last.timestamp_ms >= 4. * 1000. / 30.);
        s.worker = None;
        fs::remove_dir_all(directory.parent().ok_or("temp parent missing")?)?;
        Ok(())
    }
}
