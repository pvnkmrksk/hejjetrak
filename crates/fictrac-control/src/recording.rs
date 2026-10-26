//! Shared headless media recording, using Rust codecs and actual numerical views.
use crate::{Result, display};
use fictrac_core::{Config, DebugViews, GrayFrame, TrackRow};
use fictrac_io::{CalibratedDebugRenderer, ConfigDocument, RecordingCodec, VideoRecorder};
use std::path::Path;

/// Transient recording selection. Canonical configuration and tracker state stay unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordOptions {
    pub raw: bool,
    pub debug: bool,
}
impl RecordOptions {
    pub fn from_document(document: &ConfigDocument) -> Self {
        Self {
            raw: document.boolean("save_raw"),
            debug: document.boolean("save_debug"),
        }
    }
    /// A recording saves its effective selection without rewriting the user's document.
    pub fn recording_document(self, document: &ConfigDocument) -> Result<ConfigDocument> {
        let mut effective = document.clone();
        effective
            .set("save_raw", if self.raw { "y" } else { "n" })
            .map_err(display)?;
        effective
            .set("save_debug", if self.debug { "y" } else { "n" })
            .map_err(display)?;
        Ok(effective)
    }
}

/// Video selection is separate from the backwards-compatible raw/debug booleans.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingEncoding {
    #[default]
    Configured,
    Mjpeg,
    Raw,
}
impl RecordingEncoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Configured => "Configured codec",
            Self::Mjpeg => "MJPEG · faster, larger",
            Self::Raw => "Raw AVI · fastest, largest",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingOverflow {
    #[default]
    Backpressure,
    StopOverloaded,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MediaRecordingTuning {
    pub encoding: RecordingEncoding,
    pub overflow: RecordingOverflow,
}
impl MediaRecordingTuning {
    /// Save the explicit codec choice in the recording's effective config only.
    pub fn recording_document(self, document: &ConfigDocument) -> Result<ConfigDocument> {
        let mut effective = document.clone();
        match self.encoding {
            RecordingEncoding::Configured => {}
            RecordingEncoding::Mjpeg => effective.set("vid_codec", "mjpg").map_err(display)?,
            RecordingEncoding::Raw => effective.set("vid_codec", "raw").map_err(display)?,
        }
        Ok(effective)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaStreamState {
    #[default]
    Disabled,
    Recording,
    Complete,
    StoppedOverload,
    Failed,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MediaStreamStatus {
    pub requested: bool,
    pub state: MediaStreamState,
    /// Producer-acknowledged frames. Joined final reports contain exact counts.
    pub submitted_frames: u64,
    pub encoded_frames: u64,
    /// Includes queued, in-flight, and currently prepared producer frames.
    /// The FIFO itself is capped at two and all payloads share the byte budget.
    pub pending_frames: usize,
    pub pending_bytes: usize,
    pub queue_capacity: usize,
    pub queue_byte_capacity: usize,
    pub container_finalized: bool,
    pub output_path: Option<std::path::PathBuf>,
    pub partial_path: Option<std::path::PathBuf>,
    pub warning: Option<String>,
    pub processing_ms: f64,
    pub processing_fps: f64,
    pub elapsed_seconds: f64,
    pub first_source_frame: Option<u64>,
    pub last_source_frame: Option<u64>,
    pub first_timestamp_ms: Option<f64>,
    pub last_timestamp_ms: Option<f64>,
    pub first_encoded_source_frame: Option<u64>,
    pub last_encoded_source_frame: Option<u64>,
    pub first_encoded_timestamp_ms: Option<f64>,
    pub last_encoded_timestamp_ms: Option<f64>,
}
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct MediaRecordingStatus {
    pub encoding: RecordingEncoding,
    pub overflow: RecordingOverflow,
    pub codec: String,
    pub fps: f64,
    /// Every numeric row offered to this selected media recording, even after overload.
    pub numeric_frames: u64,
    pub raw: MediaStreamStatus,
    pub debug: MediaStreamStatus,
}

const MEDIA_QUEUE_CAPACITY: usize = 2;
const MEDIA_QUEUE_BYTE_CAPACITY: usize = 32 * 1024 * 1024;

const LIVE_VIDEO_STORAGE_RESERVE: u64 = 512 * 1024 * 1024;
#[derive(Default)]
struct StorageReserveGuard {
    checked_at: Option<std::time::Instant>,
}
impl StorageReserveGuard {
    fn check(
        &mut self,
        now: std::time::Instant,
        probe: impl FnOnce() -> Result<Option<u64>>,
    ) -> Result<()> {
        if self.checked_at.is_some_and(|last| {
            now.saturating_duration_since(last) < std::time::Duration::from_millis(100)
        }) {
            return Ok(());
        }
        self.checked_at = Some(now);
        let available = probe().map_err(|error| format!("cannot check recording storage: {error}; optional video stopped, numeric data/quality/live outputs continue"))?;
        if let Some(available) = available
            && available < LIVE_VIDEO_STORAGE_RESERVE
        {
            return Err(format!(
                "optional video stopped: only {available} bytes available, below the 512 MiB storage reserve for numeric data/quality; recorded video remains partial and live outputs continue"
            ));
        }
        Ok(())
    }
}
fn available_recording_storage(directory: &Path) -> Result<Option<u64>> {
    #[cfg(target_os = "linux")]
    {
        let status = rustix::fs::statvfs(directory).map_err(display)?;
        status
            .f_bavail
            .checked_mul(status.f_frsize)
            .filter(|_| status.f_frsize != 0)
            .map(Some)
            .ok_or_else(|| "filesystem reported invalid available storage metadata".into())
    }
    #[cfg(not(target_os = "linux"))]
    {
        // Other platforms retain ordinary encoder/filesystem error handling;
        // no unverified free-space protection is claimed for them.
        let _ = directory;
        Ok(None)
    }
}

#[derive(Clone, Copy, Default)]
struct FrameIdentity {
    frame: Option<u64>,
    timestamp_ms: Option<f64>,
}
#[derive(Default)]
struct BudgetUse {
    bytes: usize,
    frames: usize,
}
#[derive(Default)]
struct MediaBudget {
    used: std::sync::Mutex<BudgetUse>,
    changed: std::sync::Condvar,
}
struct MediaPermit {
    budget: std::sync::Arc<MediaBudget>,
    bytes: usize,
}
impl Drop for MediaPermit {
    fn drop(&mut self) {
        let mut used = self
            .budget
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        used.bytes = used.bytes.saturating_sub(self.bytes);
        used.frames = used.frames.saturating_sub(1);
        self.budget.changed.notify_all();
    }
}
struct MediaMessage<T> {
    input: T,
    identity: FrameIdentity,
    permit: MediaPermit,
}
struct ActorProgress {
    status: MediaStreamStatus,
    processing_seconds: f64,
    finished: bool,
}
struct MediaActor<T: Send + 'static> {
    sender: Option<std::sync::mpsc::SyncSender<MediaMessage<T>>>,
    worker: Option<std::thread::JoinHandle<()>>,
    progress: std::sync::Arc<std::sync::Mutex<ActorProgress>>,
    budget: std::sync::Arc<MediaBudget>,
    complete_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    overflow: RecordingOverflow,
    started: std::time::Instant,
    manifest: std::path::PathBuf,
    reported_warning: Option<String>,
}
fn actor_failure(progress: &std::sync::Mutex<ActorProgress>, message: String) {
    let mut state = progress
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.status.state = MediaStreamState::Failed;
    state.status.warning = Some(message);
}
fn actor_snapshot(
    progress: &std::sync::Mutex<ActorProgress>,
    budget: &MediaBudget,
    started: std::time::Instant,
) -> MediaStreamStatus {
    // Release progress before acquiring the byte budget (the opposite order is
    // used by a budget waiter). Neither lock covers codec, rendering or I/O.
    let (mut status, finished) = {
        let state = progress
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (state.status.clone(), state.finished)
    };
    let used = budget
        .used
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    status.pending_frames = used.frames;
    status.pending_bytes = used.bytes;
    if !finished {
        status.elapsed_seconds = started.elapsed().as_secs_f64();
    }
    status
}
fn save_media_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(display)?;
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    std::fs::write(&temporary, bytes).map_err(display)?;
    std::fs::rename(temporary, path).map_err(display)
}
impl<T: Send + 'static> MediaActor<T> {
    fn new(
        path: &Path,
        manifest: &Path,
        dimensions: [usize; 2],
        fps: f64,
        codec: RecordingCodec,
        overflow: RecordingOverflow,
        process: impl FnMut(&mut VideoRecorder, T) -> Result<()> + Send + 'static,
    ) -> Result<Self> {
        let storage_directory = path.parent().unwrap_or_else(|| Path::new(".")).to_owned();
        Self::new_with_probe(
            path,
            manifest,
            dimensions,
            fps,
            codec,
            overflow,
            (process, move || {
                available_recording_storage(&storage_directory)
            }),
        )
    }
    fn new_with_probe(
        path: &Path,
        manifest: &Path,
        dimensions: [usize; 2],
        fps: f64,
        codec: RecordingCodec,
        overflow: RecordingOverflow,
        handlers: (
            impl FnMut(&mut VideoRecorder, T) -> Result<()> + Send + 'static,
            impl FnMut() -> Result<Option<u64>> + Send + 'static,
        ),
    ) -> Result<Self> {
        let (mut process, mut storage_probe) = handlers;
        let [width, height] = dimensions;
        let mut recorder =
            VideoRecorder::create(path, width, height, fps, codec).map_err(display)?;
        let mut partial = path.as_os_str().to_os_string();
        partial.push(".partial");
        let status = MediaStreamStatus {
            requested: true,
            state: MediaStreamState::Recording,
            queue_capacity: MEDIA_QUEUE_CAPACITY,
            queue_byte_capacity: MEDIA_QUEUE_BYTE_CAPACITY,
            output_path: Some(path.to_owned()),
            partial_path: Some(partial.into()),
            ..MediaStreamStatus::default()
        };
        let progress = std::sync::Arc::new(std::sync::Mutex::new(ActorProgress {
            status,
            processing_seconds: 0.0,
            finished: false,
        }));
        let budget = std::sync::Arc::new(MediaBudget::default());
        let complete_requested = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (sender, receiver) =
            std::sync::mpsc::sync_channel::<MediaMessage<T>>(MEDIA_QUEUE_CAPACITY);
        let shared = progress.clone();
        let shared_budget = budget.clone();
        let complete = complete_requested.clone();
        let manifest_path = manifest.to_owned();
        let started = std::time::Instant::now();
        let worker = std::thread::Builder::new().name("hejjetrac-video".into()).spawn(move || {
            let mut storage_guard = StorageReserveGuard::default();
            let processing = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                while let Ok(message) = receiver.recv() {
                    let MediaMessage { input, identity, permit } = message;
                    if overflow == RecordingOverflow::StopOverloaded {
                        storage_guard.check(std::time::Instant::now(), &mut storage_probe)?;
                    }
                    let before = recorder.frame_count();
                    let tick = std::time::Instant::now();
                    let result = process(&mut recorder, input);
                    let elapsed = tick.elapsed().as_secs_f64();
                    drop(permit);
                    result?;
                    if recorder.frame_count() != before + 1 {
                        return Err("media writer did not encode exactly one frame".into());
                    }
                    let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.status.encoded_frames = recorder.frame_count();
                    state.processing_seconds += elapsed;
                    state.status.processing_ms = state.processing_seconds * 1000.0 / state.status.encoded_frames as f64;
                    state.status.processing_fps = if state.processing_seconds > 0.0 { state.status.encoded_frames as f64 / state.processing_seconds } else { 0.0 };
                    if state.status.first_encoded_source_frame.is_none() {
                        state.status.first_encoded_source_frame = identity.frame;
                        state.status.first_encoded_timestamp_ms = identity.timestamp_ms;
                    }
                    state.status.last_encoded_source_frame = identity.frame;
                    state.status.last_encoded_timestamp_ms = identity.timestamp_ms;
                }
                Ok(())
            }));
            let error = match processing {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some("video encoder worker failed unexpectedly; recorded video remains partial".into()),
            };
            if let Some(error) = error {
                actor_failure(&shared, error);
            }
            // Disconnect and release queued byte permits before codec finalization.
            drop(receiver);
            shared_budget.changed.notify_all();
            let can_complete = complete.load(std::sync::atomic::Ordering::Acquire)
                && shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner).status.state == MediaStreamState::Recording;
            if !can_complete {
                let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if state.status.state == MediaStreamState::Recording {
                    state.status.state = MediaStreamState::Failed;
                    state.status.warning = Some("video recording closed before completion; recorded prefix remains partial".into());
                }
            }
            let finalized = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if can_complete { recorder.finish() } else { recorder.finish_partial() }
            }));
            match finalized {
                Ok(Ok(())) => {
                    let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    state.status.container_finalized = true;
                    if can_complete { state.status.state = MediaStreamState::Complete; }
                }
                Ok(Err(error)) => actor_failure(&shared, format!("cannot finalize video recording: {error}")),
                Err(_) => actor_failure(&shared, "video finalization worker failed unexpectedly; recorded video remains partial".into()),
            }
            {
                let mut state = shared.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                state.finished = true;
                state.status.elapsed_seconds = started.elapsed().as_secs_f64();
            }
            let report = actor_snapshot(&shared, &shared_budget, started);
            if let Err(error) = save_media_json(&manifest_path, &report) {
                actor_failure(&shared, format!("cannot save media status: {error}"));
            }
        }).map_err(|error| format!("cannot start video encoder worker: {error}"))?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            progress,
            budget,
            complete_requested,
            overflow,
            started,
            manifest: manifest.to_owned(),
            reported_warning: None,
        })
    }
    fn status(&self) -> MediaStreamStatus {
        actor_snapshot(&self.progress, &self.budget, self.started)
    }
    fn accepting(&self) -> bool {
        self.sender.is_some() && self.status().state == MediaStreamState::Recording
    }
    fn stop_overload(&mut self, reason: String) {
        {
            let mut state = self
                .progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.status.state == MediaStreamState::Recording {
                state.status.state = MediaStreamState::StoppedOverload;
                state.status.warning = Some(reason);
            }
        }
        self.sender.take();
        self.budget.changed.notify_all();
    }
    fn submit(
        &mut self,
        bytes: usize,
        identity: FrameIdentity,
        input: impl FnOnce() -> T,
    ) -> Result<()> {
        if !self.accepting() {
            self.sender.take();
            return if self.overflow == RecordingOverflow::Backpressure {
                Err(self
                    .status()
                    .warning
                    .unwrap_or_else(|| "video encoder is stopped".into()))
            } else {
                Ok(())
            };
        }
        if bytes > MEDIA_QUEUE_BYTE_CAPACITY {
            let message = format!(
                "optional video stopped: one media frame needs {bytes} bytes, exceeding the {}-byte recording queue budget; choose a smaller capture format or data-only recording",
                MEDIA_QUEUE_BYTE_CAPACITY
            );
            self.stop_overload(message.clone());
            return if self.overflow == RecordingOverflow::Backpressure {
                Err(message)
            } else {
                Ok(())
            };
        }
        let mut used = self
            .budget
            .used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while used.bytes > MEDIA_QUEUE_BYTE_CAPACITY - bytes {
            if self.overflow == RecordingOverflow::StopOverloaded {
                drop(used);
                self.stop_overload("optional video stopped: encoder byte queue is full; numeric/quality recording continues, recorded video prefix remains .partial".into());
                return Ok(());
            }
            if self
                .progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .status
                .state
                != MediaStreamState::Recording
            {
                drop(used);
                self.sender.take();
                return Err(self.status().warning.unwrap_or_else(|| {
                    "video encoder stopped while waiting for buffer space".into()
                }));
            }
            used = self
                .budget
                .changed
                .wait(used)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        used.bytes += bytes;
        used.frames += 1;
        drop(used);
        let message = MediaMessage {
            input: input(),
            identity,
            permit: MediaPermit {
                budget: self.budget.clone(),
                bytes,
            },
        };
        let sent = match self.sender.as_ref() {
            Some(sender) if self.overflow == RecordingOverflow::Backpressure => {
                sender.send(message).map_err(|_| false)
            }
            Some(sender) => sender
                .try_send(message)
                .map_err(|error| matches!(error, std::sync::mpsc::TrySendError::Full(_))),
            None => Err(false),
        };
        match sent {
            Ok(()) => {
                let mut state = self
                    .progress
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.status.submitted_frames += 1;
                if state.status.first_source_frame.is_none() {
                    state.status.first_source_frame = identity.frame;
                    state.status.first_timestamp_ms = identity.timestamp_ms;
                }
                state.status.last_source_frame = identity.frame;
                state.status.last_timestamp_ms = identity.timestamp_ms;
                Ok(())
            }
            Err(full) => {
                if full {
                    self.stop_overload("optional video stopped: encoder queue is full; numeric/quality recording continues, recorded video prefix remains .partial; choose faster video encoding or data only".into());
                    Ok(())
                } else {
                    self.sender.take();
                    let message = self.status().warning.unwrap_or_else(|| {
                        "video encoder worker disconnected; recorded video remains partial".into()
                    });
                    actor_failure(&self.progress, message.clone());
                    if self.overflow == RecordingOverflow::Backpressure {
                        Err(message)
                    } else {
                        Ok(())
                    }
                }
            }
        }
    }
    fn take_warning(&mut self) -> Option<String> {
        let warning = self.status().warning;
        if warning == self.reported_warning {
            return None;
        }
        self.reported_warning = warning.clone();
        warning
    }
    fn finish(&mut self) {
        if self.status().state == MediaStreamState::Recording {
            self.complete_requested
                .store(true, std::sync::atomic::Ordering::Release);
        }
        self.sender.take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            actor_failure(
                &self.progress,
                "video encoder worker could not be joined; recorded video remains partial".into(),
            );
        }
        let report = self.status();
        if let Err(error) = save_media_json(&self.manifest, &report) {
            actor_failure(
                &self.progress,
                format!("cannot save final media status: {error}"),
            );
        }
    }
}
impl<T: Send + 'static> Drop for MediaActor<T> {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct DebugRecordingFrame {
    rgb: std::sync::Arc<[u8]>,
    views: DebugViews,
    path: Vec<[f64; 2]>,
    poses: Vec<[f64; 3]>,
}
pub struct MediaRecorders {
    raw: Option<MediaActor<std::sync::Arc<[u8]>>>,
    debug: Option<MediaActor<DebugRecordingFrame>>,
    config: Config,
    poses: Vec<[f64; 3]>,
    tuning: MediaRecordingTuning,
    codec: RecordingCodec,
    fps: f64,
    numeric_frames: u64,
    directory: std::path::PathBuf,
}
impl MediaRecorders {
    pub fn resolved_codec(
        doc: &ConfigDocument,
        tuning: MediaRecordingTuning,
    ) -> Result<RecordingCodec> {
        match tuning.encoding {
            RecordingEncoding::Configured => {
                RecordingCodec::from_legacy(doc.get("vid_codec")).map_err(display)
            }
            RecordingEncoding::Mjpeg => Ok(RecordingCodec::Mjpeg),
            RecordingEncoding::Raw => Ok(RecordingCodec::Raw),
        }
    }
    pub fn create(
        directory: &Path,
        doc: &ConfigDocument,
        config: &Config,
        fps: f64,
    ) -> Result<Self> {
        Self::create_with_tuning(directory, doc, config, fps, MediaRecordingTuning::default())
    }
    pub fn create_with_tuning(
        directory: &Path,
        doc: &ConfigDocument,
        config: &Config,
        fps: f64,
        tuning: MediaRecordingTuning,
    ) -> Result<Self> {
        let codec = if doc.boolean("save_raw") || doc.boolean("save_debug") {
            Self::resolved_codec(doc, tuning)?
        } else {
            RecordingCodec::Raw
        };
        let raw = if doc.boolean("save_raw") {
            Some(MediaActor::new(
                &directory.join(format!("raw.{}", codec.extension())),
                &directory.join("raw-status.json"),
                [config.width, config.height],
                fps,
                codec,
                tuning.overflow,
                |recorder, rgb: std::sync::Arc<[u8]>| recorder.write_rgb(&rgb).map_err(display),
            )?)
        } else {
            None
        };
        let debug = if doc.boolean("save_debug") {
            let renderer = CalibratedDebugRenderer::new(config, 160).map_err(display)?;
            Some(MediaActor::new(
                &directory.join(format!("debug.{}", codec.extension())),
                &directory.join("debug-status.json"),
                [640, 480],
                fps,
                codec,
                tuning.overflow,
                move |recorder, frame: DebugRecordingFrame| {
                    let image = renderer
                        .render(&frame.rgb, &frame.views, &frame.path, &frame.poses)
                        .map_err(display)?;
                    recorder.write_rgb(&image).map_err(display)
                },
            )?)
        } else {
            None
        };
        Ok(Self {
            raw,
            debug,
            config: config.clone(),
            poses: vec![],
            tuning,
            codec,
            fps,
            numeric_frames: 0,
            directory: directory.to_owned(),
        })
    }
    /// Remains true for selected streams after overload, preserving all-frame accounting.
    pub fn active(&self) -> bool {
        self.raw.is_some() || self.debug.is_some()
    }
    pub fn status(&self) -> MediaRecordingStatus {
        MediaRecordingStatus {
            encoding: self.tuning.encoding,
            overflow: self.tuning.overflow,
            codec: match self.codec {
                RecordingCodec::H264 => "h264",
                RecordingCodec::H264Mp4 => "h264_mp4",
                RecordingCodec::Xvid => "xvid",
                RecordingCodec::Mpg4 => "mpg4",
                RecordingCodec::Mjpeg => "mjpg",
                RecordingCodec::Raw => "raw",
            }
            .into(),
            fps: self.fps,
            numeric_frames: self.numeric_frames,
            raw: self
                .raw
                .as_ref()
                .map(MediaActor::status)
                .unwrap_or_default(),
            debug: self
                .debug
                .as_ref()
                .map(MediaActor::status)
                .unwrap_or_default(),
        }
    }
    pub fn take_warnings(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Some(raw) = &mut self.raw
            && let Some(warning) = raw.take_warning()
        {
            warnings.push(format!("raw video: {warning}"));
        }
        if let Some(debug) = &mut self.debug
            && let Some(warning) = debug.take_warning()
        {
            warnings.push(format!("debug video: {warning}"));
        }
        warnings
    }
    pub fn write(
        &mut self,
        rgb: &[u8],
        width: usize,
        height: usize,
        views: &DebugViews,
        path: &[[f64; 2]],
    ) -> Result<()> {
        self.numeric_frames += 1;
        self.submit(rgb, width, height, views, path, FrameIdentity::default())
    }
    fn submit(
        &mut self,
        rgb: &[u8],
        width: usize,
        height: usize,
        views: &DebugViews,
        path: &[[f64; 2]],
        identity: FrameIdentity,
    ) -> Result<()> {
        if width != self.config.width
            || height != self.config.height
            || width.checked_mul(height).and_then(|n| n.checked_mul(3)) != Some(rgb.len())
        {
            return Err("recording frame dimensions changed".into());
        }
        let rgb_shared = std::cell::OnceCell::<std::sync::Arc<[u8]>>::new();
        let mut failure = None;
        if let Some(raw) = &mut self.raw
            && let Err(error) = raw.submit(rgb.len(), identity, || {
                rgb_shared.get_or_init(|| std::sync::Arc::from(rgb)).clone()
            })
        {
            failure = Some(error);
        }
        if let Some(debug) = &mut self.debug {
            let bytes = rgb
                .len()
                .checked_add(views.roi.len())
                .and_then(|n| n.checked_add(views.diff.len()))
                .and_then(|n| n.checked_add(views.instant.len()))
                .and_then(|n| n.checked_add(views.accumulated.len()))
                .and_then(|n| path.len().checked_mul(16).and_then(|v| n.checked_add(v)))
                .and_then(|n| {
                    self.poses
                        .len()
                        .checked_mul(24)
                        .and_then(|v| n.checked_add(v))
                })
                .ok_or("debug recording buffer size overflow")?;
            if let Err(error) = debug.submit(bytes, identity, || DebugRecordingFrame {
                rgb: rgb_shared.get_or_init(|| std::sync::Arc::from(rgb)).clone(),
                views: views.clone(),
                path: path.to_vec(),
                poses: self.poses.clone(),
            }) && failure.is_none()
            {
                failure = Some(error);
            }
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    pub fn write_row(
        &mut self,
        rgb: &[u8],
        frame: &GrayFrame,
        views: &DebugViews,
        path: &[[f64; 2]],
        row: &TrackRow,
    ) -> Result<()> {
        if !row.timestamp_ms.is_finite() {
            return Err("recording timestamp must be finite".into());
        }
        if row.accepted && self.debug.as_ref().is_some_and(MediaActor::accepting) {
            if self.poses.len() >= 1000 {
                self.poses.remove(0);
            }
            self.poses.push(row.absolute_camera);
        }
        self.numeric_frames += 1;
        self.submit(
            rgb,
            frame.width(),
            frame.height(),
            views,
            path,
            FrameIdentity {
                frame: Some(row.frame),
                timestamp_ms: Some(row.timestamp_ms),
            },
        )
    }
    pub fn finish_with_status(mut self) -> (MediaRecordingStatus, Vec<String>) {
        if let Some(raw) = &mut self.raw {
            raw.finish();
        }
        if let Some(debug) = &mut self.debug {
            debug.finish();
        }
        let status = self.status();
        let mut warnings = Vec::new();
        if let Some(warning) = &status.raw.warning {
            warnings.push(format!("raw video: {warning}"));
        }
        if let Some(warning) = &status.debug.warning {
            warnings.push(format!("debug video: {warning}"));
        }
        if self.active()
            && let Err(error) = save_media_json(&self.directory.join("media-status.json"), &status)
        {
            warnings.push(format!("cannot save media recording status: {error}"));
        }
        (status, warnings)
    }
    pub fn finish(self) -> Result<()> {
        let (_, warnings) = self.finish_with_status();
        if let Some(warning) = warnings.into_iter().next() {
            Err(warning)
        } else {
            Ok(())
        }
    }
}
pub fn gray_rgb(frame: &GrayFrame) -> Vec<u8> {
    frame
        .pixels()
        .iter()
        .flat_map(|p| {
            let v = (p * 255.).round() as u8;
            [v, v, v]
        })
        .collect()
}
pub(crate) struct Recording {
    options: RecordOptions,
    dat: std::fs::File,
    all: std::fs::File,
    quality: std::fs::File,
    media: MediaRecorders,
    path: Vec<[f64; 2]>,
}
impl Recording {
    pub(crate) fn create(
        directory: &Path,
        doc: &ConfigDocument,
        config: &Config,
        fps: f64,
    ) -> Result<Self> {
        Self::create_with_tuning(directory, doc, config, fps, MediaRecordingTuning::default())
    }
    pub(crate) fn create_with_tuning(
        directory: &Path,
        doc: &ConfigDocument,
        config: &Config,
        fps: f64,
        tuning: MediaRecordingTuning,
    ) -> Result<Self> {
        use std::io::Write;
        crate::create_directory(directory)?;
        let dat = crate::create(&directory.join("tracking.dat"))?;
        let all = crate::create(&directory.join("all-frames.dat"))?;
        let mut quality = crate::create(&directory.join("quality.csv"))?;
        writeln!(
            quality,
            "frame,timestamp_ms,accepted,status,fit_error,samples"
        )
        .map_err(display)?;
        let mut file = crate::create(&directory.join("config.txt"))?;
        file.write_all(doc.text().as_bytes()).map_err(display)?;
        let media = MediaRecorders::create_with_tuning(directory, doc, config, fps, tuning)?;
        Ok(Self {
            options: RecordOptions::from_document(doc),
            dat,
            all,
            quality,
            media,
            path: vec![],
        })
    }
    pub(crate) fn options(&self) -> RecordOptions {
        self.options
    }
    pub(crate) fn media_status(&self) -> MediaRecordingStatus {
        self.media.status()
    }
    pub(crate) fn write(
        &mut self,
        row: &TrackRow,
        frame: &GrayFrame,
        rgb: &[u8],
        views: &DebugViews,
        legacy: bool,
    ) -> Result<()> {
        use std::io::Write;
        if row.accepted || !legacy {
            fictrac_io::write_dat(&mut self.dat, row).map_err(display)?;
        }
        fictrac_io::write_dat(&mut self.all, row).map_err(display)?;
        writeln!(
            self.quality,
            "{},{:.9},{},{},{:.12},{}",
            row.frame, row.timestamp_ms, row.accepted, row.status, row.error, row.samples
        )
        .map_err(display)?;
        if self.path.len() >= 100_000 {
            self.path = self.path.iter().step_by(2).copied().collect();
        }
        self.path.push(row.position);
        self.media.write_row(rgb, frame, views, &self.path, row)
    }
    pub(crate) fn finish(self) -> Result<()> {
        let mut first_error = None;
        for (name, result) in [
            ("accepted data", self.dat.sync_all()),
            ("all-frame data", self.all.sync_all()),
            ("quality evidence", self.quality.sync_all()),
        ] {
            if let Err(error) = result
                && first_error.is_none()
            {
                first_error = Some(format!("cannot flush {name}: {error}"));
            }
        }
        // A numeric flush failure must not prevent either media stream from
        // draining and finalizing its complete file or explicitly partial prefix.
        let (_, warnings) = self.media.finish_with_status();
        match first_error.or_else(|| warnings.into_iter().next()) {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod actor_tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };
    static DIRECTORY_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn directory(name: &str) -> Result<PathBuf> {
        let id = DIRECTORY_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "hejjetrac-media-actors-{}-{id}-{name}",
            std::process::id()
        ));
        std::fs::create_dir(&path).map_err(display)?;
        Ok(path)
    }
    fn identity(index: u64) -> FrameIdentity {
        FrameIdentity {
            frame: Some(100 + index),
            timestamp_ms: Some(index as f64 * 10.0),
        }
    }
    fn decode(path: &Path) -> Result<Vec<Vec<u8>>> {
        let mut source = fictrac_io::VideoSource::open(path).map_err(display)?;
        source.enable_color();
        let mut frames = Vec::new();
        while source.next_frame().map_err(display)?.is_some() {
            frames.push(source.take_rgb().ok_or("decoded media has no RGB")?);
        }
        Ok(frames)
    }
    fn actor(
        directory: &Path,
        overflow: RecordingOverflow,
        process: impl FnMut(&mut VideoRecorder, Arc<[u8]>) -> Result<()> + Send + 'static,
    ) -> Result<MediaActor<Arc<[u8]>>> {
        MediaActor::new(
            &directory.join("raw.avi"),
            &directory.join("raw-status.json"),
            [16, 16],
            100.0,
            RecordingCodec::Raw,
            overflow,
            process,
        )
    }
    #[test]
    fn storage_probe_cadence_and_errors_are_explicit() -> Result<()> {
        let start = std::time::Instant::now();
        let calls = std::cell::Cell::new(0);
        let mut guard = StorageReserveGuard::default();
        let probe = || {
            calls.set(calls.get() + 1);
            Ok(Some(LIVE_VIDEO_STORAGE_RESERVE))
        };
        guard.check(start, probe)?;
        guard.check(start + Duration::from_millis(99), probe)?;
        assert_eq!(calls.get(), 1);
        let low = guard.check(start + Duration::from_millis(100), || {
            Ok(Some(LIVE_VIDEO_STORAGE_RESERVE - 1))
        });
        assert!(low.is_err_and(|error| error.contains("512 MiB") && error.contains("numeric")));
        let mut guard = StorageReserveGuard::default();
        let failure = guard.check(start, || Err("statvfs permission denied".into()));
        assert!(
            failure.is_err_and(|error| error.contains("permission denied")
                && error.contains("optional video stopped")
                && error.contains("numeric"))
        );
        guard.check(start + Duration::from_millis(100), || Ok(None))?;
        Ok(())
    }
    #[test]
    fn lossless_actor_owns_pixels_and_drains_every_frame_in_order() -> Result<()> {
        let dir = directory("ordered")?;
        let mut recorder = MediaActor::new_with_probe(
            &dir.join("raw.avi"),
            &dir.join("raw-status.json"),
            [16, 16],
            100.0,
            RecordingCodec::Raw,
            RecordingOverflow::Backpressure,
            (
                |writer, rgb: Arc<[u8]>| writer.write_rgb(&rgb).map_err(display),
                || Err("offline recordings must not probe live storage reserve".into()),
            ),
        )?;
        let mut expected = Vec::new();
        let mut input = vec![0; 16 * 16 * 3];
        for index in 0..20 {
            input.fill(index as u8);
            expected.push(input.clone());
            recorder.submit(input.len(), identity(index), || Arc::from(input.as_slice()))?;
            input.fill(255); // Caller reuse must not change an already submitted image.
            let status = recorder.status();
            assert!(status.pending_bytes <= status.queue_byte_capacity);
            assert!(status.pending_frames <= status.queue_capacity + 1);
        }
        recorder.finish();
        let report = recorder.status();
        assert_eq!(report.state, MediaStreamState::Complete);
        assert!(report.container_finalized);
        assert_eq!(report.submitted_frames, 20);
        assert_eq!(report.encoded_frames, 20);
        assert_eq!(report.pending_frames, 0);
        assert_eq!(report.pending_bytes, 0);
        assert_eq!(report.first_encoded_source_frame, Some(100));
        assert_eq!(report.last_encoded_source_frame, Some(119));
        assert_eq!(report.last_encoded_timestamp_ms, Some(190.0));
        assert!(report.warning.is_none());
        assert_eq!(decode(&dir.join("raw.avi"))?, expected);
        assert!(!dir.join("raw.avi.partial").exists());
        std::fs::remove_dir_all(dir).map_err(display)
    }
    #[test]
    fn live_overload_never_waits_for_encoder_and_finalizes_an_explicit_prefix() -> Result<()> {
        let dir = directory("overload")?;
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut first = true;
        let mut recorder = actor(
            &dir,
            RecordingOverflow::StopOverloaded,
            move |writer, rgb| {
                if first {
                    first = false;
                    started_tx.send(()).map_err(display)?;
                    release_rx.recv().map_err(display)?;
                }
                writer.write_rgb(&rgb).map_err(display)
            },
        )?;
        recorder.submit(768, identity(0), || Arc::from(vec![0; 768]))?;
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(display)?;
        recorder.submit(768, identity(1), || Arc::from(vec![1; 768]))?;
        recorder.submit(768, identity(2), || Arc::from(vec![2; 768]))?;
        let (done_tx, done_rx) = mpsc::channel();
        let producer = thread::spawn(move || {
            let result = recorder.submit(768, identity(3), || Arc::from(vec![3; 768]));
            let _ = done_tx.send((recorder, result));
        });
        // The encoder remains blocked until after this response is received.
        let response = done_rx.recv_timeout(Duration::from_secs(2));
        release_tx.send(()).map_err(display)?;
        producer
            .join()
            .map_err(|_| "test producer failed".to_owned())?;
        let (mut recorder, result) =
            response.map_err(|_| "live media submission blocked behind its encoder".to_owned())?;
        result?;
        assert_eq!(recorder.status().state, MediaStreamState::StoppedOverload);
        assert_eq!(recorder.status().submitted_frames, 3);
        recorder.finish();
        let report = recorder.status();
        assert_eq!(report.state, MediaStreamState::StoppedOverload);
        assert!(report.container_finalized);
        assert_eq!(report.submitted_frames, report.encoded_frames);
        assert_eq!(report.pending_frames, 0);
        assert_eq!(report.pending_bytes, 0);
        assert_eq!(report.last_encoded_source_frame, Some(102));
        assert!(!dir.join("raw.avi").exists());
        assert_eq!(
            decode(&dir.join("raw.avi.partial"))?,
            vec![vec![0; 768], vec![1; 768], vec![2; 768]]
        );
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("raw-status.json")).map_err(display)?)
                .map_err(display)?;
        assert_eq!(manifest["state"], "stopped_overload");
        assert_eq!(manifest["container_finalized"], true);
        std::fs::remove_dir_all(dir).map_err(display)
    }
    #[test]
    fn worker_failure_wakes_a_lossless_producer_and_retains_partial_evidence() -> Result<()> {
        let dir = directory("failure")?;
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut recorder = actor(&dir, RecordingOverflow::Backpressure, move |_, _| {
            started_tx.send(()).map_err(display)?;
            release_rx.recv().map_err(display)?;
            Err("injected encoder failure".into())
        })?;
        recorder.submit(768, identity(0), || Arc::from(vec![0; 768]))?;
        started_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(display)?;
        recorder.submit(768, identity(1), || Arc::from(vec![1; 768]))?;
        recorder.submit(768, identity(2), || Arc::from(vec![2; 768]))?;
        let (done_tx, done_rx) = mpsc::channel();
        let producer = thread::spawn(move || {
            let result = recorder.submit(768, identity(3), || Arc::from(vec![3; 768]));
            let _ = done_tx.send((recorder, result));
        });
        assert!(done_rx.recv_timeout(Duration::from_millis(30)).is_err());
        release_tx.send(()).map_err(display)?;
        let (mut recorder, result) = done_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(display)?;
        producer
            .join()
            .map_err(|_| "test producer failed".to_owned())?;
        assert!(result.is_err());
        recorder.finish();
        let report = recorder.status();
        assert_eq!(report.state, MediaStreamState::Failed);
        assert!(
            report
                .warning
                .is_some_and(|warning| warning.contains("injected encoder failure"))
        );
        assert_eq!(report.pending_frames, 0);
        assert_eq!(report.pending_bytes, 0);
        assert!(!dir.join("raw.avi").exists());
        assert!(dir.join("raw.avi.partial").exists());
        std::fs::remove_dir_all(dir).map_err(display)
    }
    #[test]
    fn byte_budget_rejects_a_large_frame_without_allocating_its_payload() -> Result<()> {
        let dir = directory("byte-budget")?;
        let mut recorder = actor(&dir, RecordingOverflow::StopOverloaded, |writer, rgb| {
            writer.write_rgb(&rgb).map_err(display)
        })?;
        let allocated = std::cell::Cell::new(false);
        recorder.submit(MEDIA_QUEUE_BYTE_CAPACITY + 1, identity(0), || {
            allocated.set(true);
            Arc::from(vec![0; 768])
        })?;
        assert!(!allocated.get());
        assert_eq!(recorder.status().state, MediaStreamState::StoppedOverload);
        assert_eq!(recorder.status().submitted_frames, 0);
        recorder.finish();
        assert_eq!(recorder.status().pending_bytes, 0);
        std::fs::remove_dir_all(dir).map_err(display)
    }
    #[test]
    fn encoding_override_is_explicit_and_does_not_change_record_selection() -> Result<()> {
        let doc = ConfigDocument::parse("save_raw:y\nsave_debug:n\nvid_codec:h264\n".into())
            .map_err(display)?;
        let tuning = MediaRecordingTuning {
            encoding: RecordingEncoding::Mjpeg,
            ..MediaRecordingTuning::default()
        };
        let effective = tuning.recording_document(&doc)?;
        assert_eq!(doc.get("vid_codec"), Some("h264"));
        assert_eq!(effective.get("vid_codec"), Some("mjpg"));
        assert_eq!(
            RecordOptions::from_document(&doc),
            RecordOptions::from_document(&effective)
        );
        assert_eq!(
            MediaRecorders::resolved_codec(&doc, tuning)?,
            RecordingCodec::Mjpeg
        );
        Ok(())
    }
}

#[cfg(test)]
mod finalization_tests {
    use super::*;
    use std::{
        sync::{Arc, mpsc},
        time::{Duration, Instant},
    };
    #[test]
    fn low_storage_finalizes_partial_video_and_preserves_numeric_and_peer_stream() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "hejjetrac-two-streams-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(display)?
                .as_nanos()
        ));
        std::fs::create_dir(&dir).map_err(display)?;
        let config = Config::default();
        let (failed_tx, failed_rx) = mpsc::channel();
        let mut probes = 0;
        let raw = MediaActor::new_with_probe(
            &dir.join("raw.avi"),
            &dir.join("raw-status.json"),
            [config.width, config.height],
            100.0,
            RecordingCodec::Raw,
            RecordingOverflow::StopOverloaded,
            (
                move |writer, rgb: Arc<[u8]>| {
                    writer.write_rgb(&rgb).map_err(display)?;
                    failed_tx.send(()).map_err(display)
                },
                move || {
                    probes += 1;
                    Ok(Some(if probes == 1 {
                        LIVE_VIDEO_STORAGE_RESERVE + 1
                    } else {
                        LIVE_VIDEO_STORAGE_RESERVE - 1
                    }))
                },
            ),
        )?;
        let renderer = CalibratedDebugRenderer::new(&config, 160).map_err(display)?;
        let debug = MediaActor::new(
            &dir.join("debug.avi"),
            &dir.join("debug-status.json"),
            [640, 480],
            100.0,
            RecordingCodec::Raw,
            RecordingOverflow::Backpressure,
            move |writer, frame: DebugRecordingFrame| {
                let image = renderer
                    .render(&frame.rgb, &frame.views, &frame.path, &frame.poses)
                    .map_err(display)?;
                writer.write_rgb(&image).map_err(display)
            },
        )?;
        let mut media = MediaRecorders {
            raw: Some(raw),
            debug: Some(debug),
            config: config.clone(),
            poses: vec![],
            tuning: MediaRecordingTuning {
                encoding: RecordingEncoding::Raw,
                overflow: RecordingOverflow::StopOverloaded,
            },
            codec: RecordingCodec::Raw,
            fps: 100.0,
            numeric_frames: 0,
            directory: dir.clone(),
        };
        let frame = GrayFrame::new(
            config.width,
            config.height,
            vec![0.5; config.width * config.height],
            0.0,
        )
        .map_err(display)?;
        let mut tracker = fictrac_core::Tracker::new(config.clone()).map_err(display)?;
        let mut diagnostics =
            fictrac_core::Diagnostics::new(config, 6, 1.25, 0.25).map_err(display)?;
        let row = tracker.process(frame.clone()).map_err(display)?;
        let views = diagnostics.update(&frame, Some(&row));
        let rgb = gray_rgb(&frame);
        media.write_row(&rgb, &frame, &views, &[row.position], &row)?;
        failed_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(display)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while media.status().raw.encoded_frames != 1 {
            if Instant::now() > deadline {
                return Err("raw actor's first frame was not visible".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(110));
        let frame = frame.with_timestamp(10.0).map_err(display)?;
        let row = tracker.process(frame.clone()).map_err(display)?;
        let views = diagnostics.update(&frame, Some(&row));
        media.write_row(&rgb, &frame, &views, &[row.position], &row)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while media.status().raw.state != MediaStreamState::Failed {
            if Instant::now() > deadline {
                return Err("raw storage failure was not visible".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = frame.with_timestamp(20.0).map_err(display)?;
        let row = tracker.process(frame.clone()).map_err(display)?;
        let views = diagnostics.update(&frame, Some(&row));
        media.write_row(&rgb, &frame, &views, &[row.position], &row)?;
        let (status, warnings) = media.finish_with_status();
        assert_eq!(status.numeric_frames, 3);
        assert_eq!(status.raw.encoded_frames, 1);
        assert!(status.raw.container_finalized);
        assert_eq!(status.raw.pending_frames, 0);
        assert_eq!(status.raw.state, MediaStreamState::Failed);
        assert_eq!(status.debug.state, MediaStreamState::Complete);
        assert_eq!(status.debug.submitted_frames, 3);
        assert_eq!(status.debug.encoded_frames, 3);
        assert!(status.debug.container_finalized);
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("512 MiB storage reserve"))
        );
        assert!(!dir.join("raw.avi").exists());
        assert!(dir.join("raw.avi.partial").exists());
        let mut partial =
            fictrac_io::VideoSource::open(&dir.join("raw.avi.partial")).map_err(display)?;
        assert!(partial.next_frame().map_err(display)?.is_some());
        assert!(partial.next_frame().map_err(display)?.is_none());
        let mut source = fictrac_io::VideoSource::open(&dir.join("debug.avi")).map_err(display)?;
        let mut frames = 0;
        while source.next_frame().map_err(display)?.is_some() {
            frames += 1;
        }
        assert_eq!(frames, 3);
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("media-status.json")).map_err(display)?)
                .map_err(display)?;
        assert_eq!(manifest["numeric_frames"], 3);
        assert_eq!(manifest["debug"]["state"], "complete");
        std::fs::remove_dir_all(dir).map_err(display)
    }
    #[test]
    fn an_unexpected_codec_unwind_is_reported_without_poisoning_the_producer() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "hejjetrac-codec-unwind-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(display)?
                .as_nanos()
        ));
        std::fs::create_dir(&dir).map_err(display)?;
        let mut actor = MediaActor::new(
            &dir.join("raw.avi"),
            &dir.join("raw-status.json"),
            [16, 16],
            100.0,
            RecordingCodec::Raw,
            RecordingOverflow::StopOverloaded,
            |_, _: Arc<[u8]>| -> Result<()> {
                // Test-only fault injection stands in for an unexpected codec unwind.
                std::panic::resume_unwind(Box::new("injected codec unwind"))
            },
        )?;
        actor.submit(
            768,
            FrameIdentity {
                frame: Some(1),
                timestamp_ms: Some(0.0),
            },
            || Arc::from(vec![0; 768]),
        )?;
        actor.finish();
        let status = actor.status();
        assert_eq!(status.state, MediaStreamState::Failed);
        assert_eq!(status.pending_frames, 0);
        assert_eq!(status.pending_bytes, 0);
        assert!(
            status
                .warning
                .is_some_and(|w| w.contains("failed unexpectedly"))
        );
        assert!(!dir.join("raw.avi").exists());
        assert!(dir.join("raw.avi.partial").exists());
        std::fs::remove_dir_all(dir).map_err(display)
    }
}
