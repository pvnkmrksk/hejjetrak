use fictrac_control::{
    MediaRecorders, MediaRecordingStatus, MediaRecordingTuning, RecordingOverflow,
};
use fictrac_core::{DebugViews, Diagnostics, GrayFrame, TrackRow, Tracker};
use fictrac_io::{
    ConfigDocument, OutputSinks, SourceSpec, VideoSource, load_config_text, write_dat,
};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Component, Path, PathBuf},
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
    /// New source images published per second, independent of measured frames.
    pub preview_fps: f64,
    pub display_fps: u32,
    pub source_fps: f64,
    pub headroom_percent: f64,
    pub camera: Option<fictrac_camera::CameraCapabilities>,
    pub outputs: Option<fictrac_io::OutputStatus>,
    pub record_options: Option<fictrac_control::RecordOptions>,
    pub media_recording: Option<MediaRecordingStatus>,
    pub media_warnings: Vec<String>,
    pub dropped: u64,
    pub dropped_running: u64,
    pub revision: u64,
    pub recording: bool,
    pub recording_directory: Option<PathBuf>,
    /// Bounded recent history; live Stop/rotation can leave files finalizing.
    pub recording_finalizations: Vec<RecordingFinalization>,
}
#[derive(Clone, serde::Serialize)]
pub struct RecordingFinalization {
    pub directory: PathBuf,
    pub state: &'static str,
    pub numeric_error: Option<String>,
    pub media_warnings: Vec<String>,
    pub media_recording: Option<MediaRecordingStatus>,
    pub completion_error: Option<String>,
}
impl RecordingFinalization {
    fn queued(directory: PathBuf) -> Self {
        Self {
            directory,
            state: "queued",
            numeric_error: None,
            media_warnings: vec![],
            media_recording: None,
            completion_error: None,
        }
    }
    fn pending(&self) -> bool {
        matches!(self.state, "queued" | "finalizing")
    }
}
const FINALIZATION_CAPACITY: usize = 2;
const FINALIZATION_HISTORY: usize = 16;
/// A single owned worker, with a bound including both queued and running jobs.
/// No I/O or joins occur while its short metadata mutex is held.
struct RecordingFinalizer<T> {
    tx: Option<SyncSender<(PathBuf, T)>>,
    states: Arc<Mutex<Vec<RecordingFinalization>>>,
    handle: Option<thread::JoinHandle<()>>,
}
impl<T: Send + 'static> RecordingFinalizer<T> {
    fn start(finalize: impl Fn(T) -> RecordingFinalization + Send + 'static) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel::<(PathBuf, T)>(FINALIZATION_CAPACITY);
        let states = Arc::new(Mutex::new(Vec::<RecordingFinalization>::new()));
        let progress = states.clone();
        let handle = thread::Builder::new()
            .name("fictrac-recording-finalizer".into())
            .spawn(move || {
                for (directory, job) in rx {
                    {
                        let mut states = progress
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        if let Some(state) = states.iter_mut().find(|s| s.directory == directory) {
                            state.state = "finalizing";
                        }
                    }
                    let status = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| finalize(job))) {
                        Ok(status) => status,
                        Err(_) => {
                            let mut status = RecordingFinalization::queued(directory.clone());
                            status.state = "failed";
                            status.numeric_error = Some("Recording finalizer failed unexpectedly; retained files may be incomplete".into());
                            if let Err(error) = save_recording_completion(&status) { status.completion_error = Some(error); }
                            status
                        }
                    };
                    let mut states = progress
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if let Some(state) = states.iter_mut().find(|s| s.directory == directory) {
                        *state = status;
                    }
                }
            })
            .map_err(|e| format!("Cannot start recording finalizer: {e}"))?;
        Ok(Self {
            tx: Some(tx),
            states,
            handle: Some(handle),
        })
    }
    fn check_capacity(&self) -> Result<()> {
        let states = self
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if states.iter().filter(|s| s.pending()).count() >= FINALIZATION_CAPACITY {
            Err("Recording change rejected: two earlier takes are still finalizing; active recording and tracking continue. Wait for recording_finalizations to complete, then retry.".into())
        } else {
            Ok(())
        }
    }
    fn check_destination(&self, directory: &Path) -> Result<()> {
        let reserved: Vec<_> = self
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|state| state.directory.clone())
            .collect();
        let identity = recording_destination(directory)?;
        reject_reserved_destination(&identity, reserved.iter().map(PathBuf::as_path))
    }
    fn submit(&self, directory: PathBuf, job: T) -> std::result::Result<(), (String, T)> {
        if let Err(error) = self.check_capacity() {
            return Err((error, job));
        }
        if let Err(error) = self.check_destination(&directory) {
            return Err((error, job));
        }
        {
            let mut states = self
                .states
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if states.len() >= FINALIZATION_HISTORY
                && let Some(index) = states.iter().position(|s| !s.pending())
            {
                states.remove(index);
            }
            states.push(RecordingFinalization::queued(directory.clone()));
        }
        let result = match &self.tx {
            Some(tx) => tx.try_send((directory.clone(), job)).map_err(|e| match e {
                mpsc::TrySendError::Full((_, job)) => (
                    "Recording finalizer queue is full; active recording retained".into(),
                    job,
                ),
                mpsc::TrySendError::Disconnected((_, job)) => (
                    "Recording finalizer stopped; active recording retained".into(),
                    job,
                ),
            }),
            None => Err((
                "Recording finalizer is closed; active recording retained".into(),
                job,
            )),
        };
        if result.is_err() {
            self.states
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .retain(|s| s.directory != directory);
        }
        result
    }
    fn refresh(&self, snapshot: &mut Snapshot) {
        snapshot.recording_finalizations = self
            .states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        surface_recording_finalizations(snapshot);
    }
    fn drain(&mut self) -> Result<()> {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            handle.join().map_err(|_| "Recording finalizer failed unexpectedly; inspect incomplete recording directories".to_string())?;
        }
        Ok(())
    }
}
impl<T> Drop for RecordingFinalizer<T> {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
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
    RecordOptions(
        PathBuf,
        fictrac_control::RecordOptions,
        MediaRecordingTuning,
    ),
    CameraRefresh,
    CameraControl(fictrac_camera::CameraControlRequest),
    StopRecord,
    Realtime(bool),
    DisplayFps(u32),
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
    let mut finalizer = if matches!(input, Input::Camera(_)) {
        Some(RecordingFinalizer::start(Recorder::complete)?)
    } else {
        None
    };
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
        let recorder = Recorder::automatic(
            &doc,
            source_recording_tuning(&input, MediaRecordingTuning::default()),
        )?;
        snapshot.recording = true;
        snapshot.record_options = Some(fictrac_control::RecordOptions::from_document(&doc));
        snapshot.recording_directory = Some(recorder.directory.clone());
        Some(recorder)
    } else {
        None
    };
    let mut last_publish = Instant::now() - Duration::from_secs(1);
    let mut last_status_publish = Instant::now() - Duration::from_secs(1);
    let mut preview_interval = Duration::from_secs_f64(1.0 / 30.0);
    snapshot.display_fps = 30;
    let mut previous_preview: Option<Instant> = None;
    let mut wall_origin = Instant::now();
    let mut source_origin = None;
    let mut last_sequence = 0;
    let mut skip_gap = true;
    let mut playback_origin_frame = 0;
    let mut last_camera_frame = Instant::now();
    let mut last_source_time: Option<f64> = None;
    let mut retimed = false;
    let (mut sinks, mut outputs_unavailable) = match OutputSinks::from_config(&doc) {
        Ok(sinks) => (sinks, false),
        Err(error) => {
            let error = format!(
                "Live outputs are disabled: {error}. Preview, tracking and recording continue; correct the output settings and Apply to reconnect."
            );
            snapshot.notice = error.clone();
            snapshot.command_error = Some(error);
            (OutputSinks::disabled(), true)
        }
    };
    let mut previous_frame: Option<GrayFrame> = None;
    let mut previous_input: Option<GrayFrame> = None;
    let mut previous_rgb: Option<Vec<u8>> = None;
    let mut previous_debug: Option<DebugViews> = None;
    let mut pending_command = None;
    let mut pending_input = None;
    let processing_result = (|| -> Result<()> {
        'worker: while !stop.load(Ordering::Acquire) {
            for cmd in pending_command.take().into_iter().chain(rx.try_iter()) {
                match cmd {
                    Command::Configure(next, revision) => {
                        // A failed startup has no previously configured handles.
                        // Reapply even unchanged settings, e.g. after the old Python
                        // publisher releases the requested port. A disabled sink is
                        // not evidence that an enabled requested output is healthy.
                        let output_result = if outputs_unavailable {
                            OutputSinks::from_config(&next).map(|configured| {
                                sinks = configured;
                                outputs_unavailable = false;
                            })
                        } else {
                            sinks.reconfigure(&doc, &next)
                        };
                        if let Err(error) = output_result {
                            let status = sinks.status();
                            outputs_unavailable = outputs_unavailable
                                || (!status.udp_enabled
                                    && !status.serial_enabled
                                    && status.zmq.is_none());
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
                        previous_preview = None;
                        snapshot.preview_fps = 0.0;
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
                            .ok_or(
                                "Too many queued frames; wait for the current advance to finish",
                            )?;
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
                            .ok_or(
                                "Too many queued frames; wait for the current advance to finish",
                            )?;
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
                    Command::RecordOptions(dir, options, tuning) => {
                        let effective = options.recording_document(&doc)?;
                        if let Err(error) = replace_recording(
                            &mut recorder,
                            finalizer.as_ref(),
                            dir.clone(),
                            &effective,
                            source_recording_tuning(&input, tuning),
                            &mut snapshot,
                        ) {
                            recording_rejected(&mut snapshot, error);
                            continue;
                        }
                        manual_record_options = Some(options);
                        snapshot.recording = true;
                        snapshot.record_options = Some(options);
                        snapshot.media_recording = None;
                        snapshot.media_warnings.clear();
                        recording_applied(&mut snapshot, "Recording started");
                    }
                    Command::Record(dir) => {
                        if let Err(error) = replace_recording(
                            &mut recorder,
                            finalizer.as_ref(),
                            dir.clone(),
                            &doc,
                            source_recording_tuning(&input, MediaRecordingTuning::default()),
                            &mut snapshot,
                        ) {
                            recording_rejected(&mut snapshot, error);
                            continue;
                        }
                        manual_record_options = None;
                        snapshot.recording = true;
                        snapshot.record_options =
                            Some(fictrac_control::RecordOptions::from_document(&doc));
                        snapshot.media_recording = None;
                        snapshot.media_warnings.clear();
                        recording_applied(&mut snapshot, "Recording started");
                    }
                    Command::StopRecord => {
                        if let Err(error) =
                            retire_recording(&mut recorder, finalizer.as_ref(), &mut snapshot)
                        {
                            recording_rejected(&mut snapshot, error);
                            continue;
                        }
                        manual_record_options = None;
                        snapshot.recording = false;
                        snapshot.record_options = None;
                        let notice = if snapshot
                            .recording_finalizations
                            .iter()
                            .any(RecordingFinalization::pending)
                        {
                            "Recording stopped; pending takes finish in the background"
                        } else {
                            "Recording stopped; files finalized"
                        };
                        recording_applied(&mut snapshot, notice);
                    }
                    Command::Realtime(r) => {
                        realtime = r;
                        wall_origin = Instant::now();
                        source_origin = None;
                    }
                    Command::DisplayFps(fps) => {
                        if !(1..=60).contains(&fps) {
                            snapshot.command_error =
                                Some("Display FPS must be an integer between 1 and 60".into());
                            continue;
                        }
                        snapshot.display_fps = fps;
                        preview_interval = Duration::from_secs_f64(1.0 / f64::from(fps));
                        previous_preview = None;
                        snapshot.preview_fps = 0.0;
                        last_publish = Instant::now() - preview_interval;
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
                    r.tuning = source_recording_tuning(&input, r.tuning);
                    r.epoch(effective.as_ref().unwrap_or(&doc))?;
                } else if doc.boolean("save_raw") || doc.boolean("save_debug") {
                    let created = Recorder::automatic(
                        &doc,
                        source_recording_tuning(&input, MediaRecordingTuning::default()),
                    )?;
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
                            recorder
                                .write_media(&frame, &image.rgb, &debug, &path, &e.config, &row)?;
                        }
                        snapshot.image = Some(Arc::new(image));
                    }
                    snapshot.row = Some(row);
                }
            }
            if (paused && !step) || ended {
                if let Some(finalizer) = &finalizer {
                    finalizer.refresh(&mut snapshot);
                }
                if let Some(r) = &mut recorder {
                    r.refresh_media(&mut snapshot);
                }
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
                snapshot.preview_fps = 0.0;
                snapshot.advance_remaining = 0;
                snapshot.notice = "Video complete · Replay to start another sequence".into();
                if let Some(r) = &mut recorder
                    && let Some(map) = engine_state
                        .as_ref()
                        .and_then(|e| e.tracker.sphere_template())
                {
                    fictrac_io::save_sphere_map(
                        &r.directory.join(format!("epoch-{:04}-map.png", r.index)),
                        &map,
                    )
                    .map_err(|e| e.to_string())?;
                }
                retire_recording(&mut recorder, None, &mut snapshot)?;
                snapshot.recording = false;
                continue;
            };
            let raw_frame = frame.clone();
            let source_time = frame.timestamp_ms();
            let file_source = matches!(input, Input::File(_));
            let override_fps = doc.number("src_fps", -1.0);
            let frame = if file_source && (override_fps > 0.0 || retimed) {
                let time = if let (Some(previous), Some(source_prev)) =
                    (&previous_frame, last_source_time)
                {
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
                let due = Duration::try_from_secs_f64(due).map_err(
                    |_| "Invalid playback deadline; check source timestamps and src_fps",
                )?;
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
                last_publish.elapsed() >= preview_interval || step || snapshot.image.is_none();
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
                if let (Some(recorder), Some(views), Some(row)) =
                    (&mut recorder, views, row.as_ref())
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
                return Err(snapshot.notice.clone());
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
                let now = Instant::now();
                if let Some(previous) = previous_preview {
                    let elapsed = now.duration_since(previous).as_secs_f64();
                    if elapsed > 0.0 && elapsed < 2.0 {
                        let fps = 1.0 / elapsed;
                        snapshot.preview_fps = if snapshot.preview_fps > 0.0 {
                            snapshot.preview_fps * 0.85 + fps * 0.15
                        } else {
                            fps
                        };
                    }
                }
                previous_preview = Some(now);
                snapshot.path = Arc::new(path.clone());
                snapshot.contact_trace = Arc::new(contact_trace.iter().copied().collect());
                snapshot.pose_history = Arc::new(pose_history.iter().copied().collect());
                last_publish = Instant::now();
            }
            // Control/status freshness is independent of image/display cadence.
            // Reuse immutable image/path Arcs between requested image publications.
            if publish || last_status_publish.elapsed() >= Duration::from_millis(30) {
                if let Some(finalizer) = &finalizer {
                    finalizer.refresh(&mut snapshot);
                }
                if let Some(r) = &mut recorder {
                    r.refresh_media(&mut snapshot);
                }
                snapshot.outputs = Some(sinks.status());
                *shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot.clone();
                last_status_publish = Instant::now();
            }
            previous_frame = Some(frame);
            previous_input = Some(input_frame);
            step_remaining = step_remaining.saturating_sub(1);
            snapshot.advance_remaining = step_remaining;
            step = step_remaining > 0;
        }
        Ok(())
    })();
    // Acquisition has stopped before any graceful shutdown joins. This path
    // also runs after a source/output error, preserving all owned recordings.
    drop(input);
    let finalizer_result = if let Some(finalizer) = &mut finalizer {
        let result = finalizer.drain();
        finalizer.refresh(&mut snapshot);
        result
    } else {
        Ok(())
    };
    let active_result = retire_recording(&mut recorder, None, &mut snapshot);
    snapshot.recording = false;
    snapshot.record_options = None;
    *shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = snapshot;
    processing_result.and(active_result).and(finalizer_result)
}
fn replace_recording(
    recorder: &mut Option<Recorder>,
    finalizer: Option<&RecordingFinalizer<Recorder>>,
    directory: PathBuf,
    doc: &ConfigDocument,
    tuning: MediaRecordingTuning,
    snapshot: &mut Snapshot,
) -> Result<()> {
    let identity = recording_destination(&directory)?;
    reject_reserved_destination(
        &identity,
        recorder.iter().map(|active| active.directory.as_path()),
    )?;
    reject_reserved_destination(
        &identity,
        snapshot
            .recording_finalizations
            .iter()
            .map(|state| state.directory.as_path()),
    )?;
    if let Some(finalizer) = finalizer {
        finalizer.check_destination(&directory)?;
    }
    if recorder.is_some()
        && let Some(finalizer) = finalizer
    {
        finalizer.check_capacity()?;
    }
    // Invalid destinations/options must not stop an existing take.
    let next = Recorder::new_with_tuning(directory, doc, tuning)?;
    retire_recording(recorder, finalizer, snapshot)?;
    snapshot.recording_directory = Some(next.directory.clone());
    *recorder = Some(next);
    Ok(())
}
fn reject_reserved_destination<'a>(
    directory: &Path,
    reserved: impl Iterator<Item = &'a Path>,
) -> Result<()> {
    for path in reserved {
        if path == directory || recording_destination(path)? == directory {
            return Err(format!(
                "Recording destination {} is active, still finalizing, or retained in recent completion history. Choose a new unique recording directory, even if the previous directory was removed.",
                directory.display()
            ));
        }
    }
    Ok(())
}
/// Resolve existing ancestors/symlinks while retaining a stable absolute name
/// for not-yet-created or externally removed recording directories.
fn recording_destination(directory: &Path) -> Result<PathBuf> {
    let absolute = if directory.is_absolute() {
        directory.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Cannot resolve recording destination: {e}"))?
            .join(directory)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::RootDir | Component::Prefix(_) => resolved.push(component.as_os_str()),
            Component::Normal(name) => {
                resolved.push(name);
                match fs::canonicalize(&resolved) {
                    Ok(existing) => resolved = existing,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(format!(
                            "Cannot resolve recording destination {}: {error}",
                            resolved.display()
                        ));
                    }
                }
            }
        }
    }
    Ok(resolved)
}
fn recording_rejected(snapshot: &mut Snapshot, error: String) {
    let message = if error.starts_with("Recording change rejected:") {
        error
    } else {
        format!("Recording change rejected: {error}")
    };
    snapshot.command_error = Some(message.clone());
    snapshot.notice = message;
}
fn recording_applied(snapshot: &mut Snapshot, notice: &str) {
    if snapshot
        .command_error
        .as_deref()
        .is_some_and(|error| error.starts_with("Recording change rejected:"))
    {
        snapshot.command_error = None;
    }
    snapshot.notice = notice.into();
    // A retry clears its transient rejection, never an actual failed commit.
    surface_recording_finalizations(snapshot);
}
fn surface_recording_finalizations(snapshot: &mut Snapshot) {
    for state in &snapshot.recording_finalizations {
        for warning in &state.media_warnings {
            retain_media_warning(&mut snapshot.media_warnings, warning);
        }
        if let Some(error) = state
            .numeric_error
            .as_ref()
            .or(state.completion_error.as_ref())
        {
            let message = format!(
                "Recording finalization failed for {}: {error}",
                state.directory.display()
            );
            snapshot.command_error = Some(message.clone());
            snapshot.notice = message;
        }
    }
}
fn retire_recording(
    recorder: &mut Option<Recorder>,
    finalizer: Option<&RecordingFinalizer<Recorder>>,
    snapshot: &mut Snapshot,
) -> Result<()> {
    if recorder.is_some()
        && let Some(finalizer) = finalizer
    {
        finalizer.check_capacity()?;
    }
    if let Some(old) = recorder.take() {
        if let Some(finalizer) = finalizer {
            if let Err((error, old)) = finalizer.submit(old.directory.clone(), old) {
                *recorder = Some(old);
                return Err(error);
            }
            snapshot.recording = false;
            finalizer.refresh(snapshot);
        } else {
            let status = old.complete();
            snapshot.recording = false;
            snapshot.media_recording = status.media_recording.clone();
            for warning in &status.media_warnings {
                retain_media_warning(&mut snapshot.media_warnings, warning);
            }
            let error = status
                .numeric_error
                .clone()
                .or(status.completion_error.clone());
            if snapshot.recording_finalizations.len() >= FINALIZATION_HISTORY {
                snapshot.recording_finalizations.remove(0);
            }
            snapshot.recording_finalizations.push(status);
            if let Some(error) = error {
                return Err(error);
            }
        }
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
fn source_recording_tuning(
    input: &Input,
    mut tuning: MediaRecordingTuning,
) -> MediaRecordingTuning {
    tuning.overflow = if matches!(input, Input::Camera(_)) {
        RecordingOverflow::StopOverloaded
    } else {
        RecordingOverflow::Backpressure
    };
    tuning
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
    tuning: MediaRecordingTuning,
    media_status: Option<MediaRecordingStatus>,
    media_warnings: Vec<String>,
    media_disabled: bool,
    numeric_error: Option<String>,
    completion_attempted: bool,
}
impl Recorder {
    fn automatic(doc: &ConfigDocument, tuning: MediaRecordingTuning) -> Result<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let base = doc
            .get("output_fn")
            .and_then(|v| std::path::Path::new(v).file_name())
            .and_then(|v| v.to_str())
            .unwrap_or("fictrac");
        Self::new_with_tuning(
            recording_root().join(format!("{base}-{stamp}")),
            doc,
            tuning,
        )
    }
    #[cfg(test)]
    fn new(directory: PathBuf, doc: &ConfigDocument) -> Result<Self> {
        Self::new_with_tuning(directory, doc, MediaRecordingTuning::default())
    }
    fn new_with_tuning(
        directory: PathBuf,
        doc: &ConfigDocument,
        tuning: MediaRecordingTuning,
    ) -> Result<Self> {
        let effective = tuning.recording_document(doc)?;
        if let Some(p) = directory.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        fs::create_dir(&directory)
            .map_err(|e| format!("recording requires a new directory: {e}"))?;
        let data = file(&directory.join("epoch-0000.dat"))?;
        let all = file(&directory.join("epoch-0000-all-frames.dat"))?;
        let quality = file(&directory.join("epoch-0000-quality.csv"))?;
        fs::write(directory.join("epoch-0000-config.txt"), effective.text())
            .map_err(|e| e.to_string())?;
        let mut result = Self {
            directory,
            index: 0,
            data,
            all,
            quality,
            doc: effective,
            media: None,
            pending: None,
            tuning,
            media_status: None,
            media_warnings: vec![],
            media_disabled: false,
            numeric_error: None,
            completion_attempted: false,
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
        let effective = self.tuning.recording_document(doc)?;
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
            effective.text(),
        )
        .map_err(|e| e.to_string())?;
        self.doc = effective;
        self.media_disabled = false;
        self.media_status = None;
        self.header()
    }
    fn write(&mut self, row: &TrackRow) -> Result<()> {
        let result = (|| {
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
        })();
        if let Err(error) = &result {
            self.numeric_error
                .get_or_insert_with(|| format!("Cannot write numeric recording: {error}"));
        }
        result
    }
    fn start_media(&mut self, fps: f64) -> Result<()> {
        if let Some(pending) = self.pending.take() {
            let directory = self
                .directory
                .join(format!("epoch-{:04}-media", self.index));
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            let mut media = MediaRecorders::create_with_tuning(
                &directory,
                &self.doc,
                &pending.config,
                fps,
                self.tuning,
            )?;
            let result = media.write_row(
                &pending.rgb,
                &pending.frame,
                &pending.debug,
                &pending.path,
                &pending.row,
            );
            self.media = Some(media);
            result?;
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
        if self.media_disabled || (!self.doc.boolean("save_raw") && !self.doc.boolean("save_debug"))
        {
            return Ok(());
        }
        if let Some(pending) = &self.pending {
            let configured = self.doc.number("src_fps", -1.0);
            let fps = if configured > 0.0 {
                configured
            } else {
                1000.0 / (frame.timestamp_ms() - pending.frame.timestamp_ms())
            };
            if let Err(error) = self.start_media(fps) {
                self.stop_media_with_warning(error);
                return Ok(());
            }
        }
        if let Some(media) = &mut self.media {
            if let Err(error) = media.write_row(rgb, frame, debug, path, row) {
                self.stop_media_with_warning(error);
            }
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
        // Numeric evidence is committed independently of codec/renderer health.
        let mut first_error = self.numeric_error.clone();
        for (name, result) in [
            ("accepted data flush", self.data.flush()),
            ("all-frame data flush", self.all.flush()),
            ("quality flush", self.quality.flush()),
            ("accepted data sync", self.data.get_ref().sync_all()),
            ("all-frame data sync", self.all.get_ref().sync_all()),
            ("quality sync", self.quality.get_ref().sync_all()),
        ] {
            if let Err(error) = result
                && first_error.is_none()
            {
                first_error = Some(format!("Cannot finalize {name}: {error}"));
            }
        }
        if self.pending.is_some() {
            let fps = self.doc.number("src_fps", 25.0);
            if let Err(error) = self.start_media(if fps > 0.0 { fps } else { 25.0 }) {
                self.stop_media_with_warning(error);
            }
        }
        if let Some(media) = self.media.take() {
            let (status, warnings) = media.finish_with_status();
            self.media_status = Some(status);
            for warning in warnings {
                retain_media_warning(&mut self.media_warnings, &warning);
            }
        }
        self.numeric_error = first_error.clone();
        first_error.map_or(Ok(()), Err)
    }
    fn complete(mut self) -> RecordingFinalization {
        self.complete_in_place()
    }
    fn complete_in_place(&mut self) -> RecordingFinalization {
        self.completion_attempted = true;
        let numeric_error = self.finish().err();
        let mut status = RecordingFinalization {
            directory: self.directory.clone(),
            state: if numeric_error.is_some() {
                "failed"
            } else {
                "complete"
            },
            numeric_error,
            media_warnings: self.media_warnings.clone(),
            media_recording: self.media_status.clone(),
            completion_error: None,
        };
        if let Err(error) = save_recording_completion(&status) {
            status.state = "failed";
            status.completion_error = Some(error);
        }
        status
    }
    fn stop_media_with_warning(&mut self, error: String) {
        self.media_disabled = true;
        self.pending = None;
        retain_media_warning(
            &mut self.media_warnings,
            &format!("Video is partial: {error}. Numeric data, quality and live outputs continue."),
        );
    }
    fn refresh_media(&mut self, snapshot: &mut Snapshot) {
        if let Some(media) = &mut self.media {
            self.media_status = Some(media.status());
            for warning in media.take_warnings() {
                retain_media_warning(&mut self.media_warnings, &warning);
            }
        }
        snapshot.media_recording = self.media_status.clone();
        for warning in &self.media_warnings {
            retain_media_warning(&mut snapshot.media_warnings, warning);
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        if !self.completion_attempted {
            // Last-resort unwind/error cleanup owns the encoder joins too.
            // Normal paths publish the returned status through the worker.
            self.complete_in_place();
        }
    }
}
fn retain_media_warning(warnings: &mut Vec<String>, warning: &str) {
    if !warnings.iter().any(|existing| existing == warning) {
        if warnings.len() >= 16 {
            warnings.remove(0);
        }
        warnings.push(warning.into());
    }
}
fn save_recording_completion(status: &RecordingFinalization) -> Result<()> {
    let temporary = status.directory.join("completion.json.tmp");
    let destination = status.directory.join("completion.json");
    let result = (|| -> std::result::Result<(), Box<dyn std::error::Error>> {
        let bytes = serde_json::to_vec_pretty(status)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, &destination)?;
        Ok(())
    })();
    result.map_err(|e| format!("Cannot commit {}: {e}", destination.display()))
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
    struct UnblockFinalizer(SyncSender<()>);
    impl Drop for UnblockFinalizer {
        fn drop(&mut self) {
            for _ in 0..FINALIZATION_CAPACITY {
                let _ = self.0.try_send(());
            }
        }
    }
    #[test]
    fn removed_recording_destinations_remain_reserved_while_active_pending_or_recent() -> Result<()>
    {
        let (_, doc) = fixture().map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-reserved-destinations-{stamp}"));
        let mut snapshot = Snapshot::default();
        let active_directory = root.join("active");
        let mut active = Some(Recorder::new(active_directory.clone(), &doc)?);
        fs::remove_dir_all(&active_directory).map_err(|e| e.to_string())?;
        let active_alias = root.join("missing").join("..").join("active");
        let error = replace_recording(
            &mut active,
            None,
            active_alias,
            &doc,
            MediaRecordingTuning::default(),
            &mut snapshot,
        )
        .err()
        .ok_or("removed active path was reused")?;
        assert!(error.contains("new unique recording directory"));
        assert!(active.is_some());
        assert!(!active_directory.exists());
        assert!(
            !root.join("missing").exists(),
            "rejection must precede directory creation"
        );
        drop(active.take());

        let completed_directory = root.join("completed");
        let completion = Recorder::new(completed_directory.clone(), &doc)?.complete();
        assert_eq!(completion.state, "complete");
        snapshot.recording_finalizations.push(completion);
        fs::remove_dir_all(&completed_directory).map_err(|e| e.to_string())?;
        let error = replace_recording(
            &mut active,
            None,
            root.join(".").join("completed"),
            &doc,
            MediaRecordingTuning::default(),
            &mut snapshot,
        )
        .err()
        .ok_or("recent completed path was reused")?;
        assert!(error.contains("recent completion history"));
        assert!(!completed_directory.exists());
        assert_eq!(snapshot.recording_finalizations.len(), 1);
        assert_eq!(snapshot.recording_finalizations[0].state, "complete");
        assert!(
            !snapshot
                .recording_finalizations
                .iter()
                .any(RecordingFinalization::pending)
        );

        let pending_directory = root.join("pending");
        let recorder = Recorder::new(pending_directory.clone(), &doc)?;
        let (release, gate) = mpsc::sync_channel(FINALIZATION_CAPACITY);
        let (started, observed) = mpsc::sync_channel(1);
        let mut finalizer = RecordingFinalizer::start(move |recorder: Recorder| {
            let _ = started.send(());
            let _ = gate.recv();
            recorder.complete()
        })?;
        let _unblock = UnblockFinalizer(release.clone());
        finalizer
            .submit(pending_directory.clone(), recorder)
            .map_err(|(error, _)| error)?;
        observed
            .recv_timeout(Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        fs::remove_dir_all(&pending_directory).map_err(|e| e.to_string())?;
        // The snapshot deliberately lacks the queued entry: authoritative
        // finalizer ownership still prevents reuse before another status poll.
        let error = replace_recording(
            &mut active,
            Some(&finalizer),
            root.join("absent").join("..").join("pending"),
            &doc,
            MediaRecordingTuning::default(),
            &mut snapshot,
        )
        .err()
        .ok_or("removed pending path was reused")?;
        assert!(error.contains("still finalizing"));
        assert!(active.is_none());
        assert!(!pending_directory.exists());
        assert!(!root.join("absent").exists());
        finalizer.refresh(&mut snapshot);
        assert_eq!(snapshot.recording_finalizations.len(), 1);
        assert!(snapshot.recording_finalizations[0].pending());
        release.send(()).map_err(|e| e.to_string())?;
        finalizer.drain()?;
        finalizer.refresh(&mut snapshot);
        assert_eq!(snapshot.recording_finalizations.len(), 1);
        assert_eq!(snapshot.recording_finalizations[0].state, "failed");
        assert!(
            snapshot.recording_finalizations[0]
                .completion_error
                .is_some()
        );
        assert!(!pending_directory.join("completion.json").exists());
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn recording_destination_identity_resolves_symlink_ancestors_with_missing_suffix() -> Result<()>
    {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-destination-alias-{stamp}"));
        fs::create_dir_all(root.join("real")).map_err(|e| e.to_string())?;
        std::os::unix::fs::symlink(root.join("real"), root.join("alias"))
            .map_err(|e| e.to_string())?;
        let identity = recording_destination(&root.join("real").join("take"))?;
        let alias = root.join("alias").join("missing").join("..").join("take");
        assert_eq!(recording_destination(&alias)?, identity);
        assert!(reject_reserved_destination(&identity, std::iter::once(alias.as_path())).is_err());
        assert!(!root.join("real").join("take").exists());
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[test]
    fn live_rotation_keeps_processing_real_frames_with_bounded_fifo_finalization() -> Result<()> {
        let (SourceSpec::Video(input), mut doc) = fixture().map_err(|e| e.to_string())? else {
            return Err("video fixture required".into());
        };
        doc.set("save_raw", "n").map_err(|e| e.to_string())?;
        doc.set("save_debug", "n").map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-async-finalization-{stamp}"));
        let takes: Vec<_> = (0..4).map(|i| root.join(format!("take-{i}"))).collect();
        let (release, gate) = mpsc::sync_channel(FINALIZATION_CAPACITY);
        let (started, observed) = mpsc::sync_channel(FINALIZATION_CAPACITY);
        let mut finalizer = RecordingFinalizer::start(move |recorder: Recorder| {
            let _ = started.send(recorder.directory.clone());
            let _ = gate.recv();
            recorder.complete()
        })?;
        // Always release the artificial I/O gate before the owning finalizer's
        // Drop joins, including assertion/error exits from this regression.
        let _unblock = UnblockFinalizer(release.clone());
        let mut recorder = Some(Recorder::new(takes[0].clone(), &doc)?);
        let mut snapshot = Snapshot::default();
        let mut video = VideoSource::open(&input).map_err(|e| e.to_string())?;
        let mut tracker = engine(&doc, video.info.width, video.info.height, None)?
            .0
            .tracker;
        let mut clocks = vec![];
        for index in 0..8 {
            if matches!(index, 2 | 4) {
                replace_recording(
                    &mut recorder,
                    Some(&finalizer),
                    takes[index / 2].clone(),
                    &doc,
                    MediaRecordingTuning::default(),
                    &mut snapshot,
                )?;
                if index == 2 {
                    assert_eq!(
                        observed
                            .recv_timeout(Duration::from_secs(2))
                            .map_err(|e| e.to_string())?,
                        takes[0]
                    );
                }
            }
            if index == 6 {
                let error = replace_recording(
                    &mut recorder,
                    Some(&finalizer),
                    takes[3].clone(),
                    &doc,
                    MediaRecordingTuning::default(),
                    &mut snapshot,
                )
                .err()
                .ok_or("third pending rotation unexpectedly accepted")?;
                assert!(error.contains("two earlier takes"));
                assert!(
                    !takes[3].exists(),
                    "capacity rejection must not create files"
                );
                assert_eq!(
                    recorder.as_ref().ok_or("active take lost")?.directory,
                    takes[2]
                );
                assert!(retire_recording(&mut recorder, Some(&finalizer), &mut snapshot).is_err());
                assert_eq!(
                    recorder
                        .as_ref()
                        .ok_or("active take lost on Stop")?
                        .directory,
                    takes[2]
                );
            }
            let frame = video
                .next_frame()
                .map_err(|e| e.to_string())?
                .ok_or("fixture ended")?;
            let row = tracker.process(frame).map_err(|e| e.to_string())?;
            clocks.push(row.timestamp_ms);
            recorder.as_mut().ok_or("active take lost")?.write(&row)?;
        }
        assert!(clocks.windows(2).all(|pair| pair[1] > pair[0]));
        assert!(!takes[0].join("completion.json").exists());
        finalizer.refresh(&mut snapshot);
        assert_eq!(
            snapshot.recording_finalizations.len(),
            FINALIZATION_CAPACITY
        );
        assert!(
            snapshot
                .recording_finalizations
                .iter()
                .all(RecordingFinalization::pending)
        );
        // Eight real decoded/tracked frames and two rotations completed while
        // the first finalization was deterministically blocked, with no join.
        release.send(()).map_err(|e| e.to_string())?;
        assert_eq!(
            observed
                .recv_timeout(Duration::from_secs(2))
                .map_err(|e| e.to_string())?,
            takes[1]
        );
        assert!(
            takes[0].join("completion.json").exists(),
            "FIFO finalization order"
        );
        release.send(()).map_err(|e| e.to_string())?;
        finalizer.drain()?;
        finalizer.refresh(&mut snapshot);
        assert!(
            snapshot
                .recording_finalizations
                .iter()
                .all(|s| s.state == "complete")
        );
        retire_recording(&mut recorder, None, &mut snapshot)?;
        let mut identities = vec![];
        for (directory, count) in takes[..3].iter().zip([2, 2, 4]) {
            let text = fs::read_to_string(directory.join("epoch-0000-all-frames.dat"))
                .map_err(|e| e.to_string())?;
            let rows: Vec<_> = text.lines().collect();
            assert_eq!(rows.len(), count);
            for row in rows {
                identities.push(
                    row.split(',')
                        .next()
                        .ok_or("missing frame identity")?
                        .trim()
                        .parse::<u64>()
                        .map_err(|e| e.to_string())?,
                );
            }
            assert_eq!(
                fs::read_to_string(directory.join("epoch-0000-quality.csv"))
                    .map_err(|e| e.to_string())?
                    .lines()
                    .count(),
                count + 1
            );
            let marker: serde_json::Value = serde_json::from_slice(
                &fs::read(directory.join("completion.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            assert_eq!(marker["state"], "complete");
            assert!(marker["numeric_error"].is_null());
            assert!(!directory.join("completion.json.tmp").exists());
        }
        assert!(identities.windows(2).all(|pair| pair[1] == pair[0] + 1));
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[test]
    fn completion_reports_numeric_and_marker_failures_and_retains_reported_warnings() -> Result<()>
    {
        let (_, doc) = fixture().map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-finalization-errors-{stamp}"));
        let directory = root.join("numeric");
        let mut recorder = Recorder::new(directory.clone(), &doc)?;
        recorder.numeric_error = Some("injected earlier numeric write failure".into());
        recorder.stop_media_with_warning("injected media failure".into());
        recorder.refresh_media(&mut Snapshot::default());
        let status = recorder.complete();
        assert_eq!(status.state, "failed");
        assert!(status.numeric_error.is_some());
        assert_eq!(
            status.media_warnings.len(),
            1,
            "polling must not consume completion evidence"
        );
        let marker: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join("completion.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(marker["state"], "failed");
        assert_eq!(
            marker["media_warnings"]
                .as_array()
                .ok_or("missing warnings")?
                .len(),
            1
        );
        let directory = root.join("marker");
        let recorder = Recorder::new(directory.clone(), &doc)?;
        fs::create_dir(directory.join("completion.json.tmp")).map_err(|e| e.to_string())?;
        let status = recorder.complete();
        assert_eq!(status.state, "failed");
        assert!(status.numeric_error.is_none());
        assert!(
            status
                .completion_error
                .as_deref()
                .is_some_and(|e| e.contains("Cannot commit"))
        );
        assert!(!directory.join("completion.json").exists());
        let directory = root.join("media-warning");
        let mut recorder = Recorder::new(directory.clone(), &doc)?;
        recorder.stop_media_with_warning("optional stream stopped".into());
        let status = recorder.complete();
        assert_eq!(
            status.state, "complete",
            "numeric completion is independent of optional media health"
        );
        assert!(status.numeric_error.is_none());
        assert_eq!(status.media_warnings.len(), 1);
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[test]
    fn finalizer_drop_drains_owned_recorders_and_commits_completion_markers() -> Result<()> {
        let (_, doc) = fixture().map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-finalizer-drop-{stamp}"));
        let finalizer = RecordingFinalizer::start(Recorder::complete)?;
        for index in 0..2 {
            let directory = root.join(format!("take-{index}"));
            let recorder = Recorder::new(directory.clone(), &doc)?;
            finalizer
                .submit(directory, recorder)
                .map_err(|(error, _)| error)?;
        }
        drop(finalizer);
        for index in 0..2 {
            let directory = root.join(format!("take-{index}"));
            let marker: serde_json::Value = serde_json::from_slice(
                &fs::read(directory.join("completion.json")).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
            assert_eq!(marker["state"], "complete");
            assert!(!directory.join("completion.json.tmp").exists());
        }
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn wait_for(worker: &Worker, test: impl Fn(&Snapshot) -> bool) -> Result<Snapshot> {
        wait_for_timeout(worker, Duration::from_secs(15), test)
    }
    fn wait_for_timeout(
        worker: &Worker,
        timeout: Duration,
        mut test: impl FnMut(&Snapshot) -> bool,
    ) -> Result<Snapshot> {
        let start = Instant::now();
        let initial_frames = worker.latest().frames;
        let mut previous_frames = initial_frames;
        let mut last_frame_progress = start;
        loop {
            let snapshot = worker.latest();
            if snapshot.frames != previous_frames {
                previous_frames = snapshot.frames;
                last_frame_progress = Instant::now();
            }
            if test(&snapshot) {
                return Ok(snapshot);
            }
            if snapshot.ended && !snapshot.notice.contains("complete") {
                return Err(snapshot.notice);
            }
            if start.elapsed() > timeout {
                return Err(format!(
                    "worker timed out after {:?}: frames={} (advanced {}), last frame progress {:?} ago, paused={}, ended={}, revision={}, advance_remaining={}, recording={}, row={:?}, preview_updates={}, idle_waits={}, pacing_waits={}, command_error={:?}, notice={}",
                    start.elapsed(),
                    snapshot.frames,
                    snapshot.frames.saturating_sub(initial_frames),
                    last_frame_progress.elapsed(),
                    snapshot.paused,
                    snapshot.ended,
                    snapshot.revision,
                    snapshot.advance_remaining,
                    snapshot.recording,
                    snapshot
                        .row
                        .as_ref()
                        .map(|row| (row.frame, row.timestamp_ms)),
                    snapshot.resources.preview_updates,
                    snapshot.resources.idle_waits,
                    snapshot.resources.pacing_waits,
                    snapshot.command_error,
                    snapshot.notice,
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn recording_retry_clears_rejection_but_preserves_failed_completion_evidence() -> Result<()> {
        let (source, mut doc) = fixture().map_err(|e| e.to_string())?;
        doc.set("save_raw", "n").map_err(|e| e.to_string())?;
        doc.set("save_debug", "n").map_err(|e| e.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("hejjetrac-recording-retry-{stamp}"));
        let occupied = root.join("existing");
        fs::create_dir_all(&occupied).map_err(|e| e.to_string())?;
        let worker = Worker::start(source, doc, true)?;
        wait_for(&worker, |s| s.frames >= 2)?;
        worker.send(Command::Pause(true))?;
        let before = wait_for(&worker, |s| s.paused)?;
        worker.send(Command::Record(occupied.clone()))?;
        let rejected = wait_for(&worker, |s| {
            s.command_error
                .as_deref()
                .is_some_and(|e| e.starts_with("Recording change rejected:"))
        })?;
        assert_eq!(rejected.frames, before.frames);
        assert!(!rejected.recording);
        let first = root.join("first");
        worker.send(Command::Record(first.clone()))?;
        let started = wait_for(&worker, |s| {
            s.recording && s.recording_directory.as_ref() == Some(&first)
        })?;
        assert!(started.command_error.is_none());
        worker.send(Command::Advance(3))?;
        let advanced = wait_for(&worker, |s| {
            s.frames == before.frames + 3 && s.advance_remaining == 0
        })?;
        worker.send(Command::RecordOptions(
            occupied.clone(),
            fictrac_control::RecordOptions {
                raw: false,
                debug: false,
            },
            MediaRecordingTuning::default(),
        ))?;
        let rejected = wait_for(&worker, |s| {
            s.command_error
                .as_deref()
                .is_some_and(|e| e.starts_with("Recording change rejected:"))
        })?;
        assert_eq!(rejected.frames, advanced.frames);
        assert!(rejected.recording);
        assert_eq!(rejected.recording_directory.as_ref(), Some(&first));
        worker.send(Command::StopRecord)?;
        let stopped = wait_for(&worker, |s| !s.recording && s.command_error.is_none())?;
        assert_eq!(stopped.frames, advanced.frames);
        assert_eq!(
            fs::read_to_string(first.join("epoch-0000-all-frames.dat"))
                .map_err(|e| e.to_string())?
                .lines()
                .count(),
            3
        );
        assert_eq!(
            fs::read_to_string(first.join("epoch-0000-quality.csv"))
                .map_err(|e| e.to_string())?
                .lines()
                .count(),
            4
        );
        assert!(first.join("completion.json").exists());

        // A later RecordOptions retry clears an earlier Record rejection too.
        worker.send(Command::Record(occupied))?;
        wait_for(&worker, |s| {
            s.command_error
                .as_deref()
                .is_some_and(|e| e.starts_with("Recording change rejected:"))
        })?;
        let second = root.join("second");
        worker.send(Command::RecordOptions(
            second.clone(),
            fictrac_control::RecordOptions {
                raw: false,
                debug: false,
            },
            MediaRecordingTuning::default(),
        ))?;
        let started = wait_for(&worker, |s| {
            s.recording && s.recording_directory.as_ref() == Some(&second)
        })?;
        assert!(started.command_error.is_none());
        worker.send(Command::Advance(2))?;
        wait_for(&worker, |s| {
            s.frames == before.frames + 5 && s.advance_remaining == 0
        })?;
        // A real marker-write failure must survive a successful later start.
        fs::create_dir(second.join("completion.json.tmp")).map_err(|e| e.to_string())?;
        worker.send(Command::StopRecord)?;
        wait_for(&worker, |s| {
            !s.recording
                && s.recording_finalizations
                    .iter()
                    .any(|f| f.directory == second && f.state == "failed")
        })?;
        let third = root.join("third");
        worker.send(Command::Record(third.clone()))?;
        let restarted = wait_for(&worker, |s| {
            s.recording && s.recording_directory.as_ref() == Some(&third)
        })?;
        assert!(
            restarted
                .command_error
                .as_deref()
                .is_some_and(|e| e.starts_with("Recording finalization failed for "))
        );
        assert!(
            restarted
                .recording_finalizations
                .iter()
                .any(|f| f.directory == second && f.completion_error.is_some())
        );
        worker.send(Command::StopRecord)?;
        wait_for(&worker, |s| {
            !s.recording && third.join("completion.json").exists()
        })?;
        drop(worker);
        fs::remove_dir_all(root).map_err(|e| e.to_string())?;
        Ok(())
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
    fn failed_source_and_shutdown_do_not_stop_another_session() -> Result<()> {
        let (source, doc) = fixture().map_err(|e| e.to_string())?;
        let healthy = Worker::start(source, doc.clone(), true)?;
        let broken = Worker::start(
            SourceSpec::Video(PathBuf::from("/nonexistent-hejjetrac-fixture/source.mp4")),
            doc,
            true,
        )?;
        let failed = wait_for(&broken, |s| s.ended)?;
        assert_eq!(failed.frames, 0);
        assert!(failed.notice.contains("source.mp4"));
        let before = wait_for(&healthy, |s| s.frames >= 3)?.frames;
        drop(broken);
        let continuing = wait_for(&healthy, |s| s.frames > before + 2)?;
        assert!(!continuing.ended);
        assert!(continuing.command_error.is_none());
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
        let finalized = recorder.complete();
        assert_eq!(finalized.state, "complete");
        let media = finalized
            .media_recording
            .as_ref()
            .ok_or("missing finalized media status")?;
        assert!(media.raw.container_finalized && media.debug.container_finalized);
        assert_eq!(media.raw.encoded_frames, 3);
        assert_eq!(media.debug.encoded_frames, 3);
        let marker: serde_json::Value = serde_json::from_slice(
            &fs::read(directory.join("completion.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(marker["state"], "complete");
        assert_eq!(marker["media_recording"]["raw"]["encoded_frames"], 3);
        assert_eq!(marker["media_recording"]["debug"]["encoded_frames"], 3);
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
    fn occupied_startup_output_keeps_preview_and_recording_then_retries_same_settings() -> Result<()>
    {
        let busy = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let port = busy.local_addr().map_err(|e| e.to_string())?.port();
        let (source, mut doc) = fixture().map_err(|e| e.to_string())?;
        if let SourceSpec::Video(path) = &source {
            doc.set("src_fn", &path.to_string_lossy())
                .map_err(|e| e.to_string())?;
        }
        for (key, value) in [
            ("zmq_port", port.to_string()),
            ("sock_port", "-1".into()),
            ("save_raw", "n".into()),
            ("save_debug", "n".into()),
        ] {
            doc.set(key, &value).map_err(|e| e.to_string())?;
        }
        let worker = Worker::start(source, doc.clone(), true)?;
        let running = wait_for(&worker, |s| s.frames >= 3)?;
        assert!(running.image.is_some());
        assert!(running.row.is_some());
        assert!(!running.ended);
        assert!(running.command_error.as_deref().is_some_and(|s| {
            s.contains("Live outputs are disabled") && s.contains("cannot bind")
        }));
        assert!(running.outputs.as_ref().is_some_and(|s| s.zmq.is_none()));
        worker.send(Command::Pause(true))?;
        let paused = wait_for(&worker, |s| s.paused)?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("hejjetrac-busy-startup-{stamp}"));
        worker.send(Command::Record(directory.clone()))?;
        wait_for(&worker, |s| s.recording)?;
        worker.send(Command::Advance(4))?;
        let recorded = wait_for(&worker, |s| {
            s.frames == paused.frames + 4 && s.advance_remaining == 0
        })?;
        assert!(recorded.command_error.is_some());
        assert!(recorded.outputs.as_ref().is_some_and(|s| s.zmq.is_none()));
        worker.send(Command::StopRecord)?;
        wait_for(&worker, |s| !s.recording)?;
        assert_eq!(
            fs::read_to_string(directory.join("epoch-0000-all-frames.dat"))
                .map_err(|e| e.to_string())?
                .lines()
                .count(),
            4
        );
        assert_eq!(
            fs::read_to_string(directory.join("epoch-0000-quality.csv"))
                .map_err(|e| e.to_string())?
                .lines()
                .count(),
            5
        );
        worker.send(Command::Configure(doc.clone(), 1))?;
        let rejected = wait_for(&worker, |s| {
            s.command_error
                .as_deref()
                .is_some_and(|e| e.starts_with("Configuration rejected"))
        })?;
        assert_eq!(rejected.revision, 0);
        assert_eq!(rejected.frames, recorded.frames);
        drop(busy);
        worker.send(Command::Configure(doc, 2))?;
        let queued = worker.latest();
        if queued.revision < 2 {
            assert!(
                queued.command_error.is_some(),
                "A queued edit is not an acknowledgement"
            );
        }
        let applied = wait_for(&worker, |s| s.revision == 2)?;
        assert!(applied.command_error.is_none());
        assert_eq!(applied.frames, recorded.frames);
        let publisher = applied
            .outputs
            .as_ref()
            .and_then(|s| s.zmq.as_ref())
            .ok_or("Recovered publisher is missing")?;
        assert!(publisher.worker_alive);
        assert_eq!(publisher.endpoint, format!("tcp://127.0.0.1:{port}"));
        worker.send(Command::Step)?;
        let advanced = wait_for(&worker, |s| s.frames == applied.frames + 1)?;
        assert!(advanced.row.as_ref().is_some_and(|row| row.accepted));
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
            MediaRecordingTuning::default(),
        ))?;
        worker.send(Command::Realtime(false))?;
        worker.send(Command::DisplayFps(1))?;
        worker.send(Command::Pause(false))?;
        // This verifies complete persisted evidence and EOF pixels, not decode
        // throughput. Shared CI runners execute other real-video tests alongside
        // this debug-build 300-frame decode; allow that work sixty seconds while
        // keeping the separate command/deadline responsiveness assertions strict.
        let mut previous = worker.latest();
        let mut saw_independent_metadata = false;
        let ended = wait_for_timeout(&worker, Duration::from_secs(60), |snapshot| {
            if snapshot.frames > previous.frames
                && let (Some(before), Some(after)) = (&previous.image, &snapshot.image)
                && Arc::ptr_eq(before, after)
            {
                assert_eq!(
                    snapshot.resources.preview_updates,
                    previous.resources.preview_updates
                );
                assert!(Arc::ptr_eq(&previous.path, &snapshot.path));
                assert!(Arc::ptr_eq(
                    &previous.contact_trace,
                    &snapshot.contact_trace
                ));
                assert!(Arc::ptr_eq(&previous.pose_history, &snapshot.pose_history));
                saw_independent_metadata = true;
            }
            previous = snapshot.clone();
            snapshot.ended
        })?;
        assert!(
            saw_independent_metadata,
            "numeric/status counters must advance between 1-fps source images"
        );
        assert_eq!(ended.frames, 300);
        assert_eq!(ended.display_fps, 1);
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
            MediaRecordingTuning::default(),
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
