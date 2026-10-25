use fictrac_control::MediaRecorders;
use fictrac_core::{DebugViews, Diagnostics, GrayFrame, TrackRow, Tracker};
use fictrac_io::{
    ConfigDocument, OutputSinks, SourceSpec, VideoSource, load_config_text, write_dat,
};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
pub fn recording_root() -> PathBuf {
    let user = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    if let Some(user) = user {
        return PathBuf::from(user)
            .join(if cfg!(target_os = "macos") {
                "Movies"
            } else {
                "Videos"
            })
            .join("hejjetrak");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("hejjetrak-recordings")
}
fn tracking_frame(frame: GrayFrame, rgb: &[u8], doc: &ConfigDocument) -> Result<GrayFrame> {
    if doc.get("rust_engine") == Some("pairwise") {
        return Ok(frame);
    }
    let channel = doc.get("thr_rgb_tfrm").and_then(|s| match s {
        "r" | "red" => Some(0),
        "g" | "green" => Some(1),
        "b" | "blue" => Some(2),
        _ => None,
    });
    fictrac_io::rgb_tracking_frame(frame, rgb, channel).map_err(|e| e.to_string())
}
#[derive(Clone)]
pub struct ViewFrame {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    pub debug: Option<DebugViews>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    /// Actual opened video path or native camera identifier; never editor text.
    pub source_label: String,
    pub resources: WorkerResources,
    pub image: Option<Arc<ViewFrame>>,
    pub row: Option<TrackRow>,
    pub path: Arc<Vec<[f64; 2]>>,
    pub contact_trace: Arc<Vec<[f64; 3]>>,
    pub pose_history: Arc<Vec<[f64; 3]>>,
    pub frames: u64,
    pub notice: String,
    pub command_error: Option<String>,
    pub ended: bool,
    pub paused: bool,
    pub advance_remaining: usize,
    pub processing_fps: f64,
    pub processing_ms: f64,
    /// Arithmetic mean of Tracker::process only, excluding capture/conversion,
    /// engine construction, output, recording and preview publication.
    pub tracking_ms: f64,
    pub tracking_samples: u64,
    pub actual_fps: f64,
    pub source_fps: f64,
    pub headroom_percent: f64,
    pub camera: Option<fictrac_camera::CameraCapabilities>,
    pub outputs: Option<fictrac_io::OutputStatus>,
    pub record_options: Option<fictrac_control::RecordOptions>,
    pub dropped: u64,
    pub dropped_running: u64,
    pub revision: u64,
    pub recording: bool,
    pub recording_directory: Option<PathBuf>,
}
#[derive(Clone, Default, serde::Serialize)]
pub struct WorkerResources {
    /// Source images materialized; overlay-only edits do not count as new pixels.
    pub preview_updates: u64,
    /// Blocking idle/EOF wait attempts, including notifications/spurious returns.
    pub idle_waits: u64,
    /// Blocking source-deadline wait attempts; numerical frames are never skipped.
    pub pacing_waits: u64,
}
pub enum Command {
    Configure(ConfigDocument, u64),
    Pause(bool),
    Step,
    Advance(usize),
    Reset,
    LoadMap(PathBuf),
    SaveMap(PathBuf),
    Record(PathBuf),
    RecordOptions(PathBuf, fictrac_control::RecordOptions),
    CameraRefresh,
    CameraControl(fictrac_camera::CameraControlRequest),
    StopRecord,
    Realtime(bool),
}
pub struct Worker {
    tx: SyncSender<Command>,
    pub snapshot: Arc<Mutex<Snapshot>>,
    stop: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}
impl Worker {
    #[cfg(test)]
    pub fn start(source: SourceSpec, doc: ConfigDocument, realtime: bool) -> Result<Self> {
        Self::start_at(source, doc, realtime, None)
    }
    pub fn start_at(
        source: SourceSpec,
        doc: ConfigDocument,
        realtime: bool,
        config_path: Option<PathBuf>,
    ) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(16);
        let snapshot = Arc::new(Mutex::new(Snapshot {
            notice: "Opening source…".into(),
            ..Snapshot::default()
        }));
        let shared = snapshot.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let handle = thread::Builder::new()
            .name("fictrac-tracking".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(
                        source,
                        doc,
                        realtime,
                        rx,
                        &shared,
                        &stopped,
                        config_path.as_deref(),
                    )
                }));
                let error = match result {
                    Ok(Err(e)) => Some(e),
                    Err(_) => Some(
                        "Native backend failed unexpectedly; this instance stopped safely".into(),
                    ),
                    _ => None,
                };
                if let Some(e) = error {
                    let mut s = shared
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    s.notice = e;
                    s.ended = true;
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            snapshot,
            stop,
            handle: Some(handle),
        })
    }
    pub fn send(&self, command: Command) -> Result<()> {
        self.tx
            .try_send(command)
            .map_err(|e| format!("worker command not applied: {e}"))?;
        if let Some(handle) = &self.handle {
            handle.thread().unpark();
        }
        Ok(())
    }
    pub fn latest(&self) -> Snapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}
enum Input {
    File(Box<fictrac_io::PrefetchedVideoSource>),
    Camera(fictrac_camera::Camera),
}
impl Input {
    fn open(source: SourceSpec, doc: &ConfigDocument) -> Result<Self> {
        match source {
            SourceSpec::Video(path) => {
                let mut v =
                    VideoSource::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                v.enable_color();
                Ok(Self::File(Box::new(
                    v.prefetch().map_err(|e| e.to_string())?,
                )))
            }
            SourceSpec::Camera(id) => {
                let width = doc.number("rust_camera_width", 0.0);
                let height = doc.number("rust_camera_height", 0.0);
                let fps = doc.number("src_fps", -1.0);
                let camera = if width > 0.0 || height > 0.0 {
                    if !width.is_finite()
                        || !height.is_finite()
                        || width.fract() != 0.0
                        || height.fract() != 0.0
                    {
                        return Err("Camera dimensions must be finite integers".into());
                    }
                    fictrac_camera::Camera::open_with_format(
                        id,
                        fictrac_camera::CameraFormatRequest {
                            width: width as usize,
                            height: height as usize,
                            fps: if fps > 0.0 { Some(fps) } else { None },
                        },
                    )
                } else {
                    fictrac_camera::Camera::open(id, fps)
                };
                camera.map(Self::Camera).map_err(|e| e.to_string())
            }
        }
    }
    fn next(&mut self) -> Result<Option<(GrayFrame, Vec<u8>, u64)>> {
        match self {
            Self::File(v) => v.next_frame().map_err(|e| e.to_string()).map(|f| {
                f.map(|f| {
                    let rgb = v.take_rgb().unwrap_or_else(|| {
                        f.pixels()
                            .iter()
                            .flat_map(|v| [(v * 255.0) as u8; 3])
                            .collect()
                    });
                    (f, rgb, 0)
                })
            }),
            Self::Camera(c) => c
                .next_frame()
                .map_err(|e| e.to_string())
                .map(|f| f.map(|f| (f.gray, f.rgb, f.sequence))),
        }
    }
}
struct Engine {
    config: fictrac_core::Config,
    tracker: Tracker,
    diagnostics: Diagnostics,
}
fn promote_preview(
    snapshot: &mut Snapshot,
    rgb: &mut Option<Vec<u8>>,
    frame: Option<&GrayFrame>,
    engine: Option<&Engine>,
    fallback: &mut Option<DebugViews>,
) {
    if let Some(frame) = frame
        && let Some(rgb) = rgb.take()
    {
        let debug = engine
            .and_then(|engine| engine.tracker.debug_views().cloned())
            .or_else(|| fallback.take());
        snapshot.image = Some(Arc::new(ViewFrame {
            width: frame.width(),
            height: frame.height(),
            rgb,
            debug,
        }));
        snapshot.resources.preview_updates += 1;
    }
}
fn engine(
    doc: &ConfigDocument,
    w: usize,
    h: usize,
    config_path: Option<&std::path::Path>,
) -> Result<(Engine, Vec<String>)> {
    let loaded = load_config_text(doc.text(), w, h).map_err(|e| e.to_string())?;
    let mut tracker = Tracker::new(loaded.config.clone()).map_err(|e| e.to_string())?;
    if let Some(name) = doc.get("sphere_map_fn").filter(|s| !s.is_empty()) {
        let mut path = PathBuf::from(name.trim_matches('"'));
        if !path.is_absolute() && !path.exists() {
            path = config_path
                .and_then(std::path::Path::parent)
                .unwrap_or(std::path::Path::new("."))
                .join(path);
        }
        tracker
            .load_sphere_template(fictrac_io::load_sphere_map(&path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    }
    let diagnostics = Diagnostics::new(
        loaded.config.clone(),
        doc.number("q_factor", 6.0).clamp(1.0, 30.0) as usize,
        doc.number("thr_ratio", 1.25),
        doc.number("thr_win_pc", 0.25),
    )
    .map_err(|e| e.to_string())?;
    let notices = loaded.warnings;
    Ok((
        Engine {
            tracker,
            diagnostics,
            config: loaded.config,
        },
        notices,
    ))
}
fn run(
    source: SourceSpec,
    mut doc: ConfigDocument,
    mut realtime: bool,
    rx: mpsc::Receiver<Command>,
    shared: &Arc<Mutex<Snapshot>>,
    stop: &AtomicBool,
    config_path: Option<&std::path::Path>,
) -> Result<()> {
    let source_label = match &source {
        SourceSpec::Video(path) => path
            .canonicalize()
            .unwrap_or_else(|_| path.clone())
            .display()
            .to_string(),
        SourceSpec::Camera(id) => format!("camera {id}"),
    };
    let mut input = Input::open(source, &doc)?;
    let mut engine_state: Option<Engine> = None;
    let mut snapshot = Snapshot {
        source_label,
        ..Snapshot::default()
    };
    match &mut input {
        Input::Camera(camera) => {
            snapshot.source_label = camera.source_label();
            match camera.capabilities() {
                Ok(capabilities) => {
                    snapshot.source_fps = capabilities.current_format.fps.unwrap_or(0.0);
                    snapshot.camera = Some(capabilities);
                }
                Err(error) => snapshot.notice = format!("Camera controls unavailable: {error}"),
            }
        }
        Input::File(video) => {
            snapshot.source_fps = if doc.number("src_fps", -1.0) > 0.0 {
                doc.number("src_fps", -1.0)
            } else {
                video.info.fps.unwrap_or(0.0)
            }
        }
    }
    let mut last_completed: Option<Instant> = None;
    let mut last_timestamp: Option<f64> = None;
    let mut paused = false;
    let mut step = false;
    let mut step_remaining: usize = 0;
    let mut ended = false;
    let mut needs_config = true;
    let mut path = vec![];
    let mut contact_trace = VecDeque::new();
    let mut pose_history = VecDeque::new();
    let mut manual_record_options: Option<fictrac_control::RecordOptions> = None;
    let mut recorder: Option<Recorder> = if doc.boolean("save_raw") || doc.boolean("save_debug") {
        let recorder = Recorder::automatic(&doc)?;
        snapshot.recording = true;
        snapshot.record_options = Some(fictrac_control::RecordOptions::from_document(&doc));
        snapshot.recording_directory = Some(recorder.directory.clone());
        Some(recorder)
    } else {
        None
    };
    let mut last_publish = Instant::now() - Duration::from_secs(1);
    let mut wall_origin = Instant::now();
    let mut source_origin = None;
    let mut last_sequence = 0;
    let mut skip_gap = true;
    let mut playback_origin_frame = 0;
    let mut last_camera_frame = Instant::now();
    let mut last_source_time: Option<f64> = None;
    let mut retimed = false;
    let mut sinks = OutputSinks::from_config(&doc).map_err(|e| e.to_string())?;
    let mut previous_frame: Option<GrayFrame> = None;
    let mut previous_input: Option<GrayFrame> = None;
    let mut previous_rgb: Option<Vec<u8>> = None;
    let mut previous_debug: Option<DebugViews> = None;
    let mut pending_command = None;
    let mut pending_input = None;
    'worker: while !stop.load(Ordering::Acquire) {
        for cmd in pending_command.take().into_iter().chain(rx.try_iter()) {
            match cmd {
                Command::Configure(next, revision) => {
                    if let Err(error) = sinks.reconfigure(&doc, &next) {
                        snapshot.notice = format!("Configuration rejected: {error}");
                        snapshot.command_error = Some(snapshot.notice.clone());
                        continue;
                    }
                    snapshot.command_error = None;
                    if doc.number("src_fps", -1.0) != next.number("src_fps", -1.0) {
                        // A new rate starts pacing from the next emitted frame.
                        // Keep source/emitted timestamps intact: only the wall
                        // deadline anchor changes, avoiding stalls/catch-up.
                        wall_origin = Instant::now();
                        source_origin = None;
                        last_completed = None;
                    }
                    doc = next;
                    if let Input::File(video) = &input {
                        snapshot.source_fps = if doc.number("src_fps", -1.0) > 0.0 {
                            doc.number("src_fps", -1.0)
                        } else {
                            video.info.fps.unwrap_or(0.0)
                        };
                    }
                    snapshot.revision = revision;
                    needs_config = true;
                }
                Command::Pause(p) => {
                    skip_gap = true;
                    paused = p;
                    snapshot.paused = p;
                    step = false;
                    step_remaining = 0;
                    snapshot.advance_remaining = 0;
                    wall_origin = Instant::now();
                    source_origin = None;
                    last_completed = None;
                }
                Command::Advance(frames) => {
                    skip_gap = true;
                    if !(1..=10000).contains(&frames) {
                        snapshot.notice = "Advance requires 1..10000 frames".into();
                        continue;
                    }
                    step_remaining = step_remaining
                        .checked_add(frames)
                        .filter(|n| *n <= 160000)
                        .ok_or("Too many queued frames; wait for the current advance to finish")?;
                    snapshot.advance_remaining = step_remaining;
                    step = true;
                    paused = true;
                    snapshot.paused = true;
                    wall_origin = Instant::now();
                    source_origin = None;
                }
                Command::Step => {
                    skip_gap = true;
                    step_remaining = step_remaining
                        .checked_add(1)
                        .filter(|n| *n <= 160000)
                        .ok_or("Too many queued frames; wait for the current advance to finish")?;
                    snapshot.advance_remaining = step_remaining;
                    step = true;
                    paused = true;
                    snapshot.paused = true;
                    wall_origin = Instant::now();
                    source_origin = None;
                }
                Command::Reset => {
                    snapshot.tracking_ms = 0.0;
                    snapshot.tracking_samples = 0;
                    if let Some(engine) = &mut engine_state {
                        engine.tracker.reset_tracking();
                    }
                    path.clear();
                    contact_trace.clear();
                    pose_history.clear();
                    snapshot.path = Arc::new(vec![]);
                    snapshot.contact_trace = Arc::new(vec![]);
                    snapshot.pose_history = Arc::new(vec![]);
                    snapshot.row = None;
                    snapshot.notice =
                        "Pose/path reset; source counter and integrated displacement retained"
                            .into();
                }
                Command::LoadMap(filename) => {
                    let result = fictrac_io::load_sphere_map(&filename)
                        .map_err(|e| e.to_string())
                        .and_then(|map| {
                            engine_state
                                .as_mut()
                                .ok_or("No calibrated tracker".into())
                                .and_then(|engine| {
                                    engine
                                        .tracker
                                        .load_sphere_template(map)
                                        .map_err(|e| e.to_string())
                                })
                        });
                    snapshot.notice = match result {
                        Ok(()) => {
                            path.clear();
                            contact_trace.clear();
                            pose_history.clear();
                            snapshot.path = Arc::new(vec![]);
                            snapshot.contact_trace = Arc::new(vec![]);
                            snapshot.pose_history = Arc::new(vec![]);
                            snapshot.row = None;
                            "Sphere template loaded; next frame will relocalize if global search is enabled".into()
                        }
                        Err(e) => e,
                    };
                }
                Command::SaveMap(path) => {
                    let result = engine_state
                        .as_ref()
                        .and_then(|e| e.tracker.sphere_template())
                        .ok_or("No spherical map available".into())
                        .and_then(|map| {
                            fictrac_io::save_sphere_map(&path, &map).map_err(|e| e.to_string())
                        });
                    snapshot.notice = match result {
                        Ok(()) => format!("Sphere map saved: {}", path.display()),
                        Err(e) => e,
                    };
                }
                Command::CameraRefresh => {
                    snapshot.notice = match &mut input {
                        Input::Camera(camera) => match camera.capabilities() {
                            Ok(c) => {
                                snapshot.camera = Some(c);
                                "Camera controls refreshed".into()
                            }
                            Err(e) => e.to_string(),
                        },
                        _ => "Camera controls require a live camera source".into(),
                    };
                }
                Command::CameraControl(request) => {
                    snapshot.notice = match &mut input {
                        Input::Camera(camera) => match camera.set_control(request) {
                            Ok(c) => {
                                snapshot.camera = Some(c);
                                "Camera setting applied and read back".into()
                            }
                            Err(e) => e.to_string(),
                        },
                        _ => "Camera controls require a live camera source".into(),
                    };
                }
                Command::RecordOptions(dir, options) => {
                    manual_record_options = Some(options);
                    let effective = options.recording_document(&doc)?;
                    if let Some(mut old) = recorder.take() {
                        old.finish()?;
                    }
                    snapshot.recording_directory = Some(dir.clone());
                    recorder = Some(Recorder::new(dir, &effective)?);
                    snapshot.recording = true;
                    snapshot.record_options = Some(options);
                }
                Command::Record(dir) => {
                    manual_record_options = None;
                    if let Some(mut old) = recorder.take() {
                        old.finish()?;
                    }
                    snapshot.recording_directory = Some(dir.clone());
                    recorder = Some(Recorder::new(dir, &doc)?);
                    snapshot.recording = true;
                    snapshot.record_options =
                        Some(fictrac_control::RecordOptions::from_document(&doc));
                }
                Command::StopRecord => {
                    manual_record_options = None;
                    if let Some(mut r) = recorder.take() {
                        r.finish()?;
                    }
                    snapshot.recording = false;
                    snapshot.record_options = None;
                }
                Command::Realtime(r) => {
                    realtime = r;
                    wall_origin = Instant::now();
                    source_origin = None;
                }
            }
        }
        if paused || needs_config {
            promote_preview(
                &mut snapshot,
                &mut previous_rgb,
                previous_frame.as_ref(),
                engine_state.as_ref(),
                &mut previous_debug,
            );
        }
        if needs_config && let Some(frame) = &previous_frame {
            let frame = previous_input.as_ref().unwrap_or(frame);
            let frame = if let Some(image) = &snapshot.image {
                tracking_frame(frame.clone(), &image.rgb, &doc)?
            } else {
                frame.clone()
            };
            match engine(&doc, frame.width(), frame.height(), config_path) {
                Ok((e, _)) => {
                    engine_state = Some(e);
                    snapshot.notice = "Configuration applied; tracking epoch reset".into();
                }
                Err(e) => {
                    engine_state = None;
                    snapshot.notice = format!("Preview only — {e}");
                    if let Some(image) = &snapshot.image {
                        let mut image = (**image).clone();
                        image.debug = None;
                        snapshot.image = Some(Arc::new(image));
                    }
                }
            }
            path.clear();
            contact_trace.clear();
            pose_history.clear();
            snapshot.contact_trace = Arc::new(vec![]);
            snapshot.pose_history = Arc::new(vec![]);
            snapshot.path = Arc::new(vec![]);
            snapshot.row = None;
            if let Some(r) = &mut recorder {
                let effective = manual_record_options
                    .map(|o| o.recording_document(&doc))
                    .transpose()?;
                r.epoch(effective.as_ref().unwrap_or(&doc))?;
            } else if doc.boolean("save_raw") || doc.boolean("save_debug") {
                let created = Recorder::automatic(&doc)?;
                snapshot.recording = true;
                snapshot.recording_directory = Some(created.directory.clone());
                recorder = Some(created);
            }
            needs_config = false;
            // Re-evaluate a paused image immediately, without advancing the decoder.
            if let Some(e) = &mut engine_state {
                let row = e
                    .tracker
                    .process(frame.clone())
                    .map_err(|e| e.to_string())?;
                let debug = e
                    .tracker
                    .debug_views()
                    .cloned()
                    .unwrap_or_else(|| e.diagnostics.update(&frame, Some(&row)));
                if let Some(image) = &snapshot.image {
                    let mut image = (**image).clone();
                    image.debug = Some(debug.clone());
                    if let Some(recorder) = &mut recorder {
                        recorder.write(&row)?;
                        recorder.write_media(&frame, &image.rgb, &debug, &path, &e.config, &row)?;
                    }
                    snapshot.image = Some(Arc::new(image));
                }
                snapshot.row = Some(row);
            }
        }
        if (paused && !step) || ended {
            snapshot.path = Arc::new(path.clone());
            snapshot.contact_trace = Arc::new(contact_trace.iter().copied().collect());
            snapshot.pose_history = Arc::new(pose_history.iter().copied().collect());
            snapshot.outputs = Some(sinks.status());
            snapshot.resources.idle_waits += 1;
            *shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot.clone();
            // Mutating commands and shutdown unpark immediately; the long idle
            // timeout only refreshes asynchronous output health once a second.
            thread::park_timeout(Duration::from_secs(1));
            continue;
        }
        let work_tick = Instant::now();
        let frame = if let Some(frame) = pending_input.take() {
            Some(frame)
        } else {
            input.next()?
        };
        let input_wait = if matches!(input, Input::Camera(_)) {
            work_tick.elapsed()
        } else {
            Duration::ZERO
        };
        let mut pace_wait = Duration::ZERO;
        let Some((frame, rgb, sequence)) = frame else {
            if matches!(input, Input::Camera(_)) {
                if last_camera_frame.elapsed() > Duration::from_secs(5) {
                    return Err("No camera frames for five seconds. Check the device and camera permission, then Connect again.".into());
                }
                continue;
            }
            promote_preview(
                &mut snapshot,
                &mut previous_rgb,
                previous_frame.as_ref(),
                engine_state.as_ref(),
                &mut previous_debug,
            );
            snapshot.outputs = Some(sinks.status());
            snapshot.path = Arc::new(path.clone());
            snapshot.contact_trace = Arc::new(contact_trace.iter().copied().collect());
            snapshot.pose_history = Arc::new(pose_history.iter().copied().collect());
            ended = true;
            snapshot.ended = true;
            snapshot.advance_remaining = 0;
            snapshot.notice = "Video complete · Replay to start another sequence".into();
            if let Some(r) = &mut recorder {
                if let Some(map) = engine_state
                    .as_ref()
                    .and_then(|e| e.tracker.sphere_template())
                {
                    fictrac_io::save_sphere_map(
                        &r.directory.join(format!("epoch-{:04}-map.png", r.index)),
                        &map,
                    )
                    .map_err(|e| e.to_string())?;
                }
                r.finish()?;
            }
            snapshot.recording = false;
            continue;
        };
        let raw_frame = frame.clone();
        let source_time = frame.timestamp_ms();
        let file_source = matches!(input, Input::File(_));
        let override_fps = doc.number("src_fps", -1.0);
        let frame = if file_source && (override_fps > 0.0 || retimed) {
            let time =
                if let (Some(previous), Some(source_prev)) = (&previous_frame, last_source_time) {
                    previous.timestamp_ms()
                        + if override_fps > 0.0 {
                            1000.0 / override_fps
                        } else {
                            source_time - source_prev
                        }
                } else {
                    0.0
                };
            frame.with_timestamp(time).map_err(|e| e.to_string())?
        } else {
            frame
        };
        // Explicit paused advancement measures source frames immediately;
        // emitted timestamps still come from the unchanged source/FPS clock.
        if realtime && file_source && !step {
            let pacing_tick = Instant::now();
            if source_origin.is_none() {
                playback_origin_frame = snapshot.frames;
            }
            let origin = *source_origin.get_or_insert(frame.timestamp_ms());
            let due = if override_fps > 0.0 {
                snapshot.frames.saturating_sub(playback_origin_frame) as f64 / override_fps
            } else {
                (frame.timestamp_ms() - origin) / 1000.0
            };
            let due = Duration::try_from_secs_f64(due)
                .map_err(|_| "Invalid playback deadline; check source timestamps and src_fps")?;
            let deadline = wall_origin
                .checked_add(due)
                .ok_or("Playback deadline exceeds the platform clock range")?;
            while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                if stop.load(Ordering::Acquire) {
                    break 'worker;
                }
                if let Ok(command) = rx.try_recv() {
                    // Keep the unmodified decoded frame. Commands (including
                    // pause/config/FPS edits) apply before its measurement; no
                    // source clock, capture counter or numerical state advanced.
                    pending_command = Some(command);
                    pending_input = Some((raw_frame, rgb, sequence));
                    continue 'worker;
                }
                // Check the queue BEFORE parking as decoder channel waits may
                // have consumed a prior unpark token while this command waited.
                snapshot.resources.pacing_waits += 1;
                thread::park_timeout(remaining);
            }
            pace_wait = pacing_tick.elapsed();
        }
        if stop.load(Ordering::Acquire) {
            break;
        }
        if file_source {
            retimed |= override_fps > 0.0;
            last_source_time = Some(source_time);
        }
        let input_frame = frame.clone();
        let frame = tracking_frame(frame, &rgb, &doc)?;
        last_camera_frame = Instant::now();
        if sequence > 0 {
            let skipped = sequence.saturating_sub(last_sequence).saturating_sub(1);
            snapshot.dropped += skipped;
            if !skip_gap {
                snapshot.dropped_running += skipped;
            }
            skip_gap = false;
            last_sequence = sequence;
        }
        if let Some(previous) = last_timestamp {
            let interval = frame.timestamp_ms() - previous;
            if interval > 0.0 && interval < 10_000.0 && snapshot.source_fps <= 0.0 {
                snapshot.source_fps = 1000.0 / interval;
            }
        }
        last_timestamp = Some(frame.timestamp_ms());
        let tick = Instant::now();
        if needs_config {
            snapshot.tracking_ms = 0.0;
            snapshot.tracking_samples = 0;
            match engine(&doc, frame.width(), frame.height(), config_path) {
                Ok((e, _)) => {
                    snapshot.notice = if e.config.spherical_map.is_some() {
                        "Tracking · FicTrac spherical map / pure Rust BOBYQA".into()
                    } else {
                        "Tracking · photometric alternate; map panels are diagnostics".into()
                    };
                    engine_state = Some(e);
                }
                Err(e) => {
                    snapshot.notice = format!("Preview only — {e}");
                    engine_state = None;
                }
            }
            needs_config = false;
        }
        let tracking_tick = Instant::now();
        let row = if let Some(e) = &mut engine_state {
            Some(
                e.tracker
                    .process(frame.clone())
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let tracking_ms = tracking_tick.elapsed().as_secs_f64() * 1000.0;
        if row.is_some() {
            snapshot.tracking_samples += 1;
            snapshot.tracking_ms +=
                (tracking_ms - snapshot.tracking_ms) / snapshot.tracking_samples as f64;
        }
        snapshot.processing_fps = 1.0 / tick.elapsed().as_secs_f64().max(1e-9);
        snapshot.frames += 1;
        snapshot.row = row.clone();
        let mut output_error = None;
        if let Some(row) = &row {
            if row.accepted
                && let Some(engine) = &engine_state
            {
                let lab = glam::DQuat::from_scaled_axis(glam::DVec3::from_array(
                    engine.config.camera_to_lab,
                ));
                let rotation =
                    glam::DQuat::from_scaled_axis(glam::DVec3::from_array(row.absolute_camera));
                let material = rotation.inverse() * (lab.inverse() * -glam::DVec3::Z);
                contact_trace.push_back(material.to_array());
                pose_history.push_back(row.absolute_camera);
                if contact_trace.len() > 1000 {
                    contact_trace.pop_front();
                    pose_history.pop_front();
                }
            }
            path.push(row.position);
            if path.len() > 20_000 {
                path = path.iter().step_by(2).copied().collect();
            }
            if let Some(r) = &mut recorder
                && let Err(error) = r.write(row)
            {
                output_error = Some(error);
            }
            for error in sinks.write(row) {
                snapshot.notice = format!("Tracking continues; {error}");
            }
        }
        let publish =
            last_publish.elapsed() > Duration::from_millis(30) || step || snapshot.image.is_none();
        let mut debug = None;
        if let Some(engine) = &mut engine_state {
            // Preserve pairwise diagnostic accumulation on every measured frame.
            // Map mode already retains its numerical views; borrow them for
            // recording and copy only the image actually published to the GUI.
            let fallback = if engine.tracker.debug_views().is_none() {
                Some(engine.diagnostics.update(&frame, row.as_ref()))
            } else {
                None
            };
            let views = engine.tracker.debug_views().or(fallback.as_ref());
            if let (Some(recorder), Some(views), Some(row)) = (&mut recorder, views, row.as_ref())
                && output_error.is_none()
                && let Err(error) =
                    recorder.write_media(&frame, &rgb, views, &path, &engine.config, row)
            {
                output_error = Some(error);
            }
            if publish {
                debug = views.cloned();
            } else {
                previous_debug = fallback;
            }
        }
        if publish {
            snapshot.image = Some(Arc::new(ViewFrame {
                width: frame.width(),
                height: frame.height(),
                rgb,
                debug,
            }));
            snapshot.resources.preview_updates += 1;
            previous_rgb = None;
            previous_debug = None;
        } else {
            // Exactly one unpublished RGB frame is retained, so paused edits
            // use its original pixels without cloning every full-frame image.
            previous_rgb = Some(rgb);
        }
        if let Some(error) = output_error {
            promote_preview(
                &mut snapshot,
                &mut previous_rgb,
                Some(&frame),
                engine_state.as_ref(),
                &mut previous_debug,
            );
            snapshot.outputs = Some(sinks.status());
            snapshot.path = Arc::new(path.clone());
            snapshot.contact_trace = Arc::new(contact_trace.iter().copied().collect());
            snapshot.pose_history = Arc::new(pose_history.iter().copied().collect());
            snapshot.notice = format!(
                "Recording failed after measured frame {}; partial files retained: {error}",
                snapshot.frames
            );
            snapshot.ended = true;
            *shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot.clone();
            return Err(snapshot.notice);
        }
        let now = Instant::now();
        if let Some(previous) = last_completed {
            let dt = now.duration_since(previous).as_secs_f64();
            if dt > 0.0 && dt < 2.0 {
                let fps = 1.0 / dt;
                snapshot.actual_fps = if snapshot.actual_fps > 0.0 {
                    snapshot.actual_fps * 0.85 + fps * 0.15
                } else {
                    fps
                };
            }
        }
        last_completed = Some(now);
        let elapsed = work_tick
            .elapsed()
            .saturating_sub(input_wait)
            .saturating_sub(pace_wait)
            .as_secs_f64()
            * 1000.0;
        snapshot.processing_ms = if snapshot.processing_ms > 0.0 {
            snapshot.processing_ms * 0.85 + elapsed * 0.15
        } else {
            elapsed
        };
        snapshot.headroom_percent = if snapshot.source_fps > 0.0 {
            100.0 * (1.0 - snapshot.processing_ms * snapshot.source_fps / 1000.0)
        } else {
            0.0
        };
        if publish {
            snapshot.outputs = Some(sinks.status());
            snapshot.path = Arc::new(path.clone());
            snapshot.contact_trace = Arc::new(contact_trace.iter().copied().collect());
            snapshot.pose_history = Arc::new(pose_history.iter().copied().collect());
            *shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot.clone();
            last_publish = Instant::now();
        }
        previous_frame = Some(frame);
        previous_input = Some(input_frame);
        step_remaining = step_remaining.saturating_sub(1);
        snapshot.advance_remaining = step_remaining;
        step = step_remaining > 0;
    }
    if let Some(mut r) = recorder {
        r.finish()?;
    }
    Ok(())
}
struct PendingMedia {
    rgb: Vec<u8>,
    config: fictrac_core::Config,
    debug: DebugViews,
    path: Vec<[f64; 2]>,
    frame: GrayFrame,
    row: TrackRow,
}
struct Recorder {
    directory: PathBuf,
    index: u32,
    data: BufWriter<File>,
    all: BufWriter<File>,
    quality: BufWriter<File>,
    doc: ConfigDocument,
    media: Option<MediaRecorders>,
    pending: Option<PendingMedia>,
}
impl Recorder {
    fn automatic(doc: &ConfigDocument) -> Result<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let base = doc
            .get("output_fn")
            .and_then(|v| std::path::Path::new(v).file_name())
            .and_then(|v| v.to_str())
            .unwrap_or("fictrac");
        Self::new(recording_root().join(format!("{base}-{stamp}")), doc)
    }
    fn new(directory: PathBuf, doc: &ConfigDocument) -> Result<Self> {
        if let Some(p) = directory.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        fs::create_dir(&directory)
            .map_err(|e| format!("recording requires a new directory: {e}"))?;
        let data = file(&directory.join("epoch-0000.dat"))?;
        let all = file(&directory.join("epoch-0000-all-frames.dat"))?;
        let quality = file(&directory.join("epoch-0000-quality.csv"))?;
        fs::write(directory.join("epoch-0000-config.txt"), doc.text())
            .map_err(|e| e.to_string())?;
        let mut result = Self {
            directory,
            index: 0,
            data,
            all,
            quality,
            doc: doc.clone(),
            media: None,
            pending: None,
        };
        result.header()?;
        Ok(result)
    }
    fn header(&mut self) -> Result<()> {
        writeln!(self.quality, "frame,timestamp_ms,accepted,status,fit_error")
            .map_err(|e| e.to_string())
    }
    fn epoch(&mut self, doc: &ConfigDocument) -> Result<()> {
        self.finish()?;
        self.index = self
            .index
            .checked_add(1)
            .ok_or("recording epoch overflow")?;
        self.data = file(&self.directory.join(format!("epoch-{:04}.dat", self.index)))?;
        self.all = file(
            &self
                .directory
                .join(format!("epoch-{:04}-all-frames.dat", self.index)),
        )?;
        self.quality = file(
            &self
                .directory
                .join(format!("epoch-{:04}-quality.csv", self.index)),
        )?;
        fs::write(
            self.directory
                .join(format!("epoch-{:04}-config.txt", self.index)),
            doc.text(),
        )
        .map_err(|e| e.to_string())?;
        self.doc = doc.clone();
        self.header()
    }
    fn write(&mut self, row: &TrackRow) -> Result<()> {
        if row.accepted {
            write_dat(&mut self.data, row).map_err(|e| e.to_string())?;
        }
        write_dat(&mut self.all, row).map_err(|e| e.to_string())?;
        writeln!(
            self.quality,
            "{},{},{},{},{}",
            row.frame, row.timestamp_ms, row.accepted, row.status, row.error
        )
        .map_err(|e| e.to_string())
    }
    fn start_media(&mut self, fps: f64) -> Result<()> {
        if let Some(pending) = self.pending.take() {
            let directory = self
                .directory
                .join(format!("epoch-{:04}-media", self.index));
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            let mut media = MediaRecorders::create(&directory, &self.doc, &pending.config, fps)?;
            media.write_row(
                &pending.rgb,
                &pending.frame,
                &pending.debug,
                &pending.path,
                &pending.row,
            )?;
            self.media = Some(media);
        }
        Ok(())
    }
    fn write_media(
        &mut self,
        frame: &GrayFrame,
        rgb: &[u8],
        debug: &DebugViews,
        path: &[[f64; 2]],
        config: &fictrac_core::Config,
        row: &TrackRow,
    ) -> Result<()> {
        if !self.doc.boolean("save_raw") && !self.doc.boolean("save_debug") {
            return Ok(());
        }
        if let Some(pending) = &self.pending {
            let configured = self.doc.number("src_fps", -1.0);
            let fps = if configured > 0.0 {
                configured
            } else {
                1000.0 / (frame.timestamp_ms() - pending.frame.timestamp_ms())
            };
            self.start_media(fps)?;
        }
        if let Some(media) = &mut self.media {
            media.write_row(rgb, frame, debug, path, row)?;
        } else {
            self.pending = Some(PendingMedia {
                rgb: rgb.to_vec(),
                config: config.clone(),
                debug: debug.clone(),
                path: path.to_vec(),
                frame: frame.clone(),
                row: row.clone(),
            });
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        if self.pending.is_some() {
            let fps = self.doc.number("src_fps", 25.0);
            self.start_media(if fps > 0.0 { fps } else { 25.0 })?;
        }
        if let Some(media) = self.media.take() {
            media.finish()?;
        }
        self.data
            .flush()
            .and_then(|_| self.all.flush())
            .and_then(|_| self.quality.flush())
            .map_err(|e| e.to_string())?;
        self.data
            .get_ref()
            .sync_all()
            .and_then(|_| self.all.get_ref().sync_all())
            .and_then(|_| self.quality.get_ref().sync_all())
            .map_err(|e| e.to_string())
    }
}
fn file(path: &PathBuf) -> Result<BufWriter<File>> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(BufWriter::new)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    fn fixture() -> std::result::Result<(SourceSpec, ConfigDocument), Box<dyn std::error::Error>> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let doc = ConfigDocument::parse(fs::read_to_string(
            root.join("examples/upstream-config.txt"),
        )?)?;
        Ok((SourceSpec::Video(root.join("examples/sample.mp4")), doc))
    }
    fn wait_for(worker: &Worker, test: impl Fn(&Snapshot) -> bool) -> Result<Snapshot> {
        let start = Instant::now();
        loop {
            let snapshot = worker.latest();
            if test(&snapshot) {
                return Ok(snapshot);
            }
            if snapshot.ended && !snapshot.notice.contains("complete") {
                return Err(snapshot.notice);
            }
            if start.elapsed() > Duration::from_secs(15) {
                return Err(format!("worker timed out: {}", snapshot.notice));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn pause_step_reconfigure_and_diagnostics_are_independent() -> Result<()> {
        let (source, doc) = fixture().map_err(|e| e.to_string())?;
        let a = Worker::start(source.clone(), doc.clone(), true)?;
        let b = Worker::start(source, doc.clone(), true)?;
        wait_for(&a, |s| s.frames >= 3)?;
        a.send(Command::Pause(true))?;
        let paused = wait_for(&a, |s| s.paused)?;
        let before = paused.frames;
        a.send(Command::Step)?;
        let step = wait_for(&a, |s| s.frames == before + 1)?;
        assert!(step.paused);
        assert!(step.image.as_ref().is_some_and(|i| i.debug.is_some()));
        a.send(Command::Configure(doc, 4))?;
        let configured = wait_for(&a, |s| s.revision == 4)?;
        assert_eq!(
            configured.row.as_ref().map(|r| r.status),
            Some("initializing")
        );
        let other = wait_for(&b, |s| s.frames > before + 1)?;
        assert!(!other.paused);
        assert_eq!(other.revision, 0);
        Ok(())
    }
    #[test]
    fn four_video_sources_run_without_shared_state() -> Result<()> {
        let (source, doc) = fixture().map_err(|e| e.to_string())?;
        let workers: Vec<_> = (0..4)
            .map(|_| Worker::start(source.clone(), doc.clone(), true))
            .collect::<Result<_>>()?;
        for worker in &workers {
            let s = wait_for(worker, |s| s.frames >= 3)?;
            assert!(s.row.is_some());
            assert_eq!(s.dropped, 0);
        }
        Ok(())
    }
    #[test]
    fn malformed_config_keeps_a_responsive_video_preview() -> Result<()> {
        let (source, mut doc) = fixture().map_err(|e| e.to_string())?;
        doc.set("vfov", "invalid").map_err(|e| e.to_string())?;
        let worker = Worker::start(source, doc, true)?;
        let snapshot = wait_for(&worker, |s| s.frames >= 2)?;
        assert!(snapshot.row.is_none());
        assert!(snapshot.image.is_some());
        assert!(snapshot.notice.contains("Preview only"));
        Ok(())
    }
    #[test]
    fn udp_rows_use_the_legacy_prefix_and_order() -> Result<()> {
        let receiver = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        receiver
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let (source, mut doc) = fixture().map_err(|e| e.to_string())?;
        doc.set("sock_host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        doc.set(
            "sock_port",
            &receiver
                .local_addr()
                .map_err(|e| e.to_string())?
                .port()
                .to_string(),
        )
        .map_err(|e| e.to_string())?;
        let _worker = Worker::start(source, doc, true)?;
        let mut bytes = [0; 4096];
        for frame in 0..3 {
            let n = receiver.recv(&mut bytes).map_err(|e| e.to_string())?;
            let row = std::str::from_utf8(&bytes[..n]).map_err(|e| e.to_string())?;
            let values: Vec<_> = row
                .strip_prefix("FT, ")
                .ok_or("missing wire prefix")?
                .trim()
                .split(',')
                .collect();
            assert_eq!(values.len(), 25);
            assert_eq!(values[0].trim(), frame.to_string());
        }
        Ok(())
    }
    #[test]
    fn desktop_and_headless_default_map_results_agree() -> Result<()> {
        let (source, doc) = fixture().map_err(|e| e.to_string())?;
        let worker = Worker::start(source.clone(), doc.clone(), true)?;
        let snapshot = wait_for(&worker, |s| s.frames >= 4)?;
        drop(worker);
        let SourceSpec::Video(path) = source else {
            return Err("video fixture required".into());
        };
        let mut video = VideoSource::open(&path).map_err(|e| e.to_string())?;
        let config = load_config_text(doc.text(), 384, 288)
            .map_err(|e| e.to_string())?
            .config;
        let channel = config
            .spherical_map
            .as_ref()
            .ok_or("map mode required")?
            .channel;
        let mut tracker = Tracker::new(config).map_err(|e| e.to_string())?;
        let mut expected = None;
        for _ in 0..snapshot.frames {
            let frame = video
                .next_tracking_frame(channel)
                .map_err(|e| e.to_string())?
                .ok_or("fixture ended early")?;
            expected = Some(tracker.process(frame).map_err(|e| e.to_string())?);
        }
        let expected = expected.ok_or("no expected row")?;
        let actual = snapshot.row.ok_or("no desktop row")?;
        assert_eq!(expected.frame, actual.frame);
        assert_eq!(expected.delta_camera, actual.delta_camera);
        assert_eq!(expected.position, actual.position);
        assert_eq!(expected.error, actual.error);
        Ok(())
    }
    #[test]
    fn queued_advances_accumulate_and_report_completion() -> Result<()> {
        let (source, doc) = fixture().map_err(|e| e.to_string())?;
        let worker = Worker::start(source, doc, true)?;
        wait_for(&worker, |s| s.frames >= 3)?;
        worker.send(Command::Pause(true))?;
        let before = wait_for(&worker, |s| s.paused)?.frames;
        worker.send(Command::Advance(2))?;
        worker.send(Command::Advance(3))?;
        let after = wait_for(&worker, |s| {
            s.frames >= before + 5 && s.advance_remaining == 0
        })?;
        assert_eq!(after.frames, before + 5);
        assert!(after.paused);
        Ok(())
    }
    #[test]
    fn desktop_recording_keeps_first_frame_and_finalizes_playable_media() -> Result<()> {
        let (source, mut doc) = fixture().map_err(|e| e.to_string())?;
        doc.set("save_raw", "y").map_err(|e| e.to_string())?;
        doc.set("save_debug", "y").map_err(|e| e.to_string())?;
        doc.set("vid_codec", "mjpg").map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("fictrac-desktop-record-{stamp}"));
        let mut recorder = Recorder::new(directory.clone(), &doc)?;
        let SourceSpec::Video(input) = source else {
            return Err("video fixture required".into());
        };
        let mut video = VideoSource::open(&input).map_err(|e| e.to_string())?;
        let config = load_config_text(doc.text(), 384, 288)
            .map_err(|e| e.to_string())?
            .config;
        let mut tracker = Tracker::new(config.clone()).map_err(|e| e.to_string())?;
        let mut path = vec![];
        for _ in 0..3 {
            let frame = video
                .next_tracking_frame(None)
                .map_err(|e| e.to_string())?
                .ok_or("no frame")?;
            let rgb = video.take_rgb().ok_or("no color frame")?;
            let row = tracker.process(frame.clone()).map_err(|e| e.to_string())?;
            path.push(row.position);
            recorder.write(&row)?;
            recorder.write_media(
                &frame,
                &rgb,
                tracker.debug_views().ok_or("no diagnostics")?,
                &path,
                &config,
                &row,
            )?;
        }
        recorder.finish()?;
        for (name, width, height) in [("raw.avi", 384, 288), ("debug.avi", 640, 480)] {
            let mut result = VideoSource::open(&directory.join("epoch-0000-media").join(name))
                .map_err(|e| e.to_string())?;
            assert_eq!((result.info.width, result.info.height), (width, height));
            assert!((result.info.fps.ok_or("no fps")? - 30.0).abs() < 1e-8);
            let mut count = 0;
            while result.next_frame().map_err(|e| e.to_string())?.is_some() {
                count += 1;
            }
            assert_eq!(count, 3);
        }
        assert_eq!(
            fs::read_to_string(directory.join("epoch-0000-all-frames.dat"))
                .map_err(|e| e.to_string())?
                .lines()
                .count(),
            3
        );
        fs::remove_dir_all(directory).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[test]
    fn occupied_publisher_edit_keeps_native_tracker_source_and_queue_alive() -> Result<()> {
        let busy = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let (source, doc) = fixture().map_err(|e| e.to_string())?;
        let worker = Worker::start(source, doc.clone(), true)?;
        wait_for(&worker, |s| s.frames >= 3)?;
        worker.send(Command::Pause(true))?;
        let before = wait_for(&worker, |s| s.paused)?;
        let previous = before.row.as_ref().ok_or("Missing measured row")?;
        let mut invalid = doc.clone();
        invalid
            .set(
                "zmq_port",
                &busy
                    .local_addr()
                    .map_err(|e| e.to_string())?
                    .port()
                    .to_string(),
            )
            .map_err(|e| e.to_string())?;
        worker.send(Command::Configure(invalid, 9))?;
        let rejected = wait_for(&worker, |s| s.command_error.is_some())?;
        assert_eq!(rejected.frames, before.frames);
        assert_eq!(rejected.revision, 0);
        assert!(!rejected.ended);
        assert!(rejected.notice.contains("rollback succeeded"));
        worker.send(Command::Step)?;
        let advanced = wait_for(&worker, |s| s.frames == before.frames + 1)?;
        let row = advanced
            .row
            .as_ref()
            .ok_or("Missing row after rejected output edit")?;
        assert_eq!(row.frame, previous.frame + 1);
        assert_eq!(row.sequence, previous.sequence + 1);
        assert!(row.timestamp_ms > previous.timestamp_ms);
        worker.send(Command::Configure(doc, 10))?;
        let restored = wait_for(&worker, |s| s.revision == 10)?;
        assert!(restored.command_error.is_none());
        Ok(())
    }
    #[test]
    fn commands_interrupt_long_source_wait_without_losing_staged_pixels_or_clock() -> Result<()> {
        let (source, mut doc) = fixture().map_err(|error| error.to_string())?;
        doc.set("src_fps", "0.1")
            .map_err(|error| error.to_string())?;
        let worker = Worker::start(source.clone(), doc.clone(), true)?;
        wait_for(&worker, |snapshot| snapshot.frames >= 1)?;
        let command_tick = Instant::now();
        worker.send(Command::Pause(true))?;
        let paused = wait_for(&worker, |snapshot| snapshot.paused)?;
        assert!(command_tick.elapsed() < Duration::from_secs(2));
        assert_eq!(paused.frames, 1);
        let image = paused.image.as_ref().ok_or("missing paused image")?;
        thread::sleep(Duration::from_millis(150));
        let still = worker.latest();
        assert_eq!(still.frames, paused.frames);
        assert!(still.resources.idle_waits <= paused.resources.idle_waits + 2);
        assert!(Arc::ptr_eq(
            image,
            still.image.as_ref().ok_or("paused image disappeared")?
        ));

        doc.set("src_fps", "2").map_err(|error| error.to_string())?;
        worker.send(Command::Configure(doc, 1))?;
        wait_for(&worker, |snapshot| snapshot.revision == 1)?;
        worker.send(Command::Step)?;
        let stepped = wait_for(&worker, |snapshot| snapshot.frames == 2)?;
        assert_eq!(
            stepped
                .row
                .as_ref()
                .ok_or("missing stepped row")?
                .timestamp_ms,
            500.0
        );
        let SourceSpec::Video(path) = source else {
            return Err("video fixture required".into());
        };
        let mut video = VideoSource::open(&path).map_err(|error| error.to_string())?;
        video.enable_color();
        video.next_frame().map_err(|error| error.to_string())?;
        video.next_frame().map_err(|error| error.to_string())?;
        assert_eq!(
            stepped.image.as_ref().ok_or("missing stepped image")?.rgb,
            video.take_rgb().ok_or("missing expected second image")?
        );
        let drop_tick = Instant::now();
        drop(worker);
        assert!(drop_tick.elapsed() < Duration::from_secs(2));
        Ok(())
    }
    #[test]
    fn publication_throttle_retains_final_image_and_every_recorded_quality_row() -> Result<()> {
        let (source, mut doc) = fixture().map_err(|error| error.to_string())?;
        doc.set("src_fps", "0.1")
            .map_err(|error| error.to_string())?;
        let worker = Worker::start(source.clone(), doc, true)?;
        wait_for(&worker, |snapshot| snapshot.frames >= 1)?;
        worker.send(Command::Pause(true))?;
        wait_for(&worker, |snapshot| snapshot.paused)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("hejjetrak-resource-rows-{stamp}"));
        worker.send(Command::RecordOptions(
            directory.clone(),
            fictrac_control::RecordOptions {
                raw: false,
                debug: false,
            },
        ))?;
        worker.send(Command::Realtime(false))?;
        worker.send(Command::Pause(false))?;
        let ended = wait_for(&worker, |snapshot| snapshot.ended)?;
        assert_eq!(ended.frames, 300);
        assert!(ended.resources.preview_updates > 0);
        assert!(ended.resources.preview_updates <= ended.frames);
        assert_eq!(ended.row.as_ref().ok_or("missing final row")?.frame, 299);
        assert_eq!(
            fs::read_to_string(directory.join("epoch-0000-all-frames.dat"))
                .map_err(|error| error.to_string())?
                .lines()
                .count(),
            299
        );
        assert_eq!(
            fs::read_to_string(directory.join("epoch-0000-quality.csv"))
                .map_err(|error| error.to_string())?
                .lines()
                .count(),
            300
        );
        let SourceSpec::Video(path) = source else {
            return Err("video fixture required".into());
        };
        let mut video = VideoSource::open(&path).map_err(|error| error.to_string())?;
        video.enable_color();
        let mut last_rgb = None;
        while video
            .next_frame()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            last_rgb = video.take_rgb();
        }
        assert_eq!(
            ended.image.as_ref().ok_or("missing EOF image")?.rgb,
            last_rgb.ok_or("missing expected EOF image")?
        );
        drop(worker);
        fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
        Ok(())
    }
    #[test]
    fn repeated_manual_steps_ignore_playback_wait_and_preserve_source_clock() -> Result<()> {
        let (source, mut doc) = fixture().map_err(|error| error.to_string())?;
        doc.set("src_fps", "0.1")
            .map_err(|error| error.to_string())?;
        let worker = Worker::start(source, doc, true)?;
        wait_for(&worker, |snapshot| snapshot.frames >= 1)?;
        worker.send(Command::Pause(true))?;
        wait_for(&worker, |snapshot| snapshot.paused)?;
        for frames in 2..=3 {
            let tick = Instant::now();
            worker.send(Command::Step)?;
            let snapshot = wait_for(&worker, |snapshot| snapshot.frames == frames)?;
            assert!(tick.elapsed() < Duration::from_secs(2));
            assert!(snapshot.paused);
            assert_eq!(
                snapshot
                    .row
                    .as_ref()
                    .ok_or("missing manual-step row")?
                    .timestamp_ms,
                (frames - 1) as f64 * 10_000.0
            );
        }
        Ok(())
    }
    #[test]
    fn running_rate_changes_reanchor_without_stall_or_lost_recorded_rows() -> Result<()> {
        // Small decoded video keeps debug-build work below a120FPS deadline;
        // the real H264 sample remains covered by the other worker regressions.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("hejjetrak-rate-change-{stamp}"));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let input = directory.join("clock.y4m");
        let mut bytes = b"YUV4MPEG2 W64 H64 F120:1 Ip Cmono\n".to_vec();
        for _ in 0..400 {
            bytes.extend_from_slice(b"FRAME\n");
            bytes.extend(
                (0..4096).map(|pixel| (((pixel % 64 / 2 + pixel / 64 / 2) % 2) * 255) as u8),
            );
        }
        fs::write(&input, bytes).map_err(|error| error.to_string())?;
        let mut doc = ConfigDocument::parse("src_fps:120\nvfov:90\nroi_c:{0,0,1}\nroi_r:0.5\nc2a_r:{0,0,0}\nq_factor:2\nopt_max_evals:16\nopt_do_global:n\nopt_max_err:-1\nrust_resilient:n\n".into()).map_err(|error| error.to_string())?;
        // Validate actual source geometry before starting the worker. A16px
        // image has too few visible sites for the shared diagnostic sampler.
        engine(&doc, 64, 64, None)?;
        let worker = Worker::start(SourceSpec::Video(input), doc.clone(), true)?;
        wait_for(&worker, |snapshot| snapshot.frames >= 3)?;
        worker.send(Command::Pause(true))?;
        let before = wait_for(&worker, |snapshot| snapshot.paused)?;
        let recording = directory.join("recording");
        worker.send(Command::RecordOptions(
            recording.clone(),
            fictrac_control::RecordOptions {
                raw: false,
                debug: false,
            },
        ))?;
        worker.send(Command::Pause(false))?;
        let fast = wait_for(&worker, |snapshot| snapshot.frames >= before.frames + 60)?;
        let fast_time = fast.row.as_ref().ok_or("missing120FPS row")?.timestamp_ms;
        doc.set("src_fps", "30")
            .map_err(|error| error.to_string())?;
        let tick = Instant::now();
        worker.send(Command::Configure(doc.clone(), 1))?;
        let slow = wait_for(&worker, |snapshot| {
            snapshot.revision == 1 && snapshot.frames > fast.frames
        })?;
        assert!(
            tick.elapsed() < Duration::from_secs(1),
            "120→30 must not wait on the old rate's origin"
        );
        assert!(slow.row.as_ref().ok_or("missing30FPS row")?.timestamp_ms > fast_time);
        let slow = wait_for(&worker, |snapshot| snapshot.frames >= slow.frames + 30)?;
        let slow_time = slow
            .row
            .as_ref()
            .ok_or("missing later30FPS row")?
            .timestamp_ms;
        doc.set("src_fps", "120")
            .map_err(|error| error.to_string())?;
        let tick = Instant::now();
        worker.send(Command::Configure(doc, 2))?;
        let fast_again = wait_for(&worker, |snapshot| {
            snapshot.revision == 2 && snapshot.frames > slow.frames
        })?;
        assert!(tick.elapsed() < Duration::from_secs(1));
        assert!(
            fast_again
                .row
                .as_ref()
                .ok_or("missing resumed120FPS row")?
                .timestamp_ms
                > slow_time
        );
        worker.send(Command::Pause(true))?;
        let after = wait_for(&worker, |snapshot| snapshot.paused)?;
        worker.send(Command::StopRecord)?;
        wait_for(&worker, |snapshot| !snapshot.recording)?;
        drop(worker);
        let mut all_rows = 0;
        let mut quality_rows = 0;
        for entry in fs::read_dir(&recording).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with("-all-frames.dat") {
                let text = fs::read_to_string(entry.path()).map_err(|error| error.to_string())?;
                let timestamps = text
                    .lines()
                    .map(|line| {
                        line.split(',')
                            .nth(21)
                            .ok_or("missing timestamp")?
                            .trim()
                            .parse::<f64>()
                            .map_err(|error| error.to_string())
                    })
                    .collect::<Result<Vec<_>>>()?;
                assert!(timestamps.windows(2).all(|pair| pair[1] > pair[0]));
                all_rows += timestamps.len();
            } else if name.ends_with("-quality.csv") {
                quality_rows += fs::read_to_string(entry.path())
                    .map_err(|error| error.to_string())?
                    .lines()
                    .count()
                    - 1;
            }
        }
        // Each reconfiguration starts a separately logged epoch, including its
        // initialization on the previous image without advancing source count.
        assert_eq!(all_rows as u64, after.frames - before.frames + 2);
        assert_eq!(quality_rows, all_rows);
        fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
        Ok(())
    }
}
