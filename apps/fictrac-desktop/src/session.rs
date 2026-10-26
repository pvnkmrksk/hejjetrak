use crate::runtime::{Command, Snapshot, Worker};
use eframe::egui;
use fictrac_io::{ConfigDocument, DocumentHistory, SourceSpec};
use std::{fs, path::PathBuf, time::Instant};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Inspect,
    Rim,
    Exclude,
    Square,
    Edit,
}

pub struct Session {
    pub source_base: Option<PathBuf>,
    pub running_source: String,
    pub running_fps: f64,
    pub id: u64,
    pub history: DocumentHistory,
    pub path: Option<PathBuf>,
    pub saved: String,
    pub disk_conflict: bool,
    pub worker: Option<Worker>,
    pub snapshot: Snapshot,
    pub error: String,
    pub source_edit: String,
    pub show_settings: bool,
    pub show_config_text: bool,
    pub show_about: bool,
    pub show_rigs: bool,
    pub show_diagnostics: bool,
    pub show_overlays: bool,
    pub display_fps: u32,
    pub cameras: Vec<fictrac_camera::CameraInfo>,
    pub occupied_cameras: std::collections::BTreeMap<u32, String>,
    pub camera_edit: Option<fictrac_camera::CameraFormatRequest>,
    pub camera_error: String,
    pub record_raw: bool,
    pub record_debug: bool,
    pub recording_encoding: fictrac_control::RecordingEncoding,
    pub startup_fallback: Option<ConfigDocument>,
    pub fallback_notice: String,
    pub temporary_config: bool,
    pub paused: bool,
    pub realtime: bool,
    pub tool: Tool,
    pub edit_existing: bool,
    pub calibration_review: bool,
    pub calibration_redraw: bool,
    pub calibration_previous_paused: Option<bool>,
    pub magnifier: bool,
    pub points: Vec<[f64; 2]>,
    pub square_slots: Option<[Option<[f64; 2]>; 4]>,
    pub window_rect: Option<[f32; 4]>,
    pub window_inner_size: Option<[f32; 2]>,
    pub framebuffer_limit: u32,
    pub max_window_inner_size: egui::Vec2,
    pub requested_window_size: Option<egui::Vec2>,
    pub calibration_preview_size: Option<egui::Vec2>,
    pub last_title: String,
    pub plane: String,
    pub full_image: bool,
    pub changed: Option<Instant>,
    pub revision: u64,
    pub textures: Vec<egui::TextureHandle>,
    pub texture_frame: u64,
    pub calibration_cache: std::cell::RefCell<Option<CalibrationCache>>,
    pub texture_image: Option<std::sync::Arc<crate::runtime::ViewFrame>>,
    pub spawn_request: usize,
    pub closed: bool,
    pub native_window: bool,
    pub close_pending: bool,
    pub last_disk_check: Instant,
    pub status: String,
    pub auto_apply: bool,
    pub pending_drag: Option<(usize, usize)>,
    pub recording_path: String,
    pub zmq_resume_port: Option<i32>,
}
impl Session {
    pub fn new(id: u64, doc: ConfigDocument, path: Option<PathBuf>) -> Self {
        let text = doc.text().to_owned();
        let source_edit = doc.get("src_fn").unwrap_or("").to_owned();
        Self {
            source_base: path.clone(),
            running_source: String::new(),
            running_fps: -1.0,
            id,
            history: DocumentHistory::new(text.clone()),
            path,
            saved: text,
            disk_conflict: false,
            worker: None,
            snapshot: Snapshot::default(),
            error: String::new(),
            source_edit,
            show_settings: false,
            show_config_text: false,
            show_about: false,
            show_rigs: false,
            show_diagnostics: false,
            show_overlays: false,
            display_fps: 30,
            cameras: fictrac_camera::list_cameras().unwrap_or_default(),
            occupied_cameras: std::collections::BTreeMap::new(),
            camera_edit: None,
            camera_error: String::new(),
            zmq_resume_port: None,
            record_raw: doc.boolean("save_raw"),
            record_debug: doc.boolean("save_debug"),
            recording_encoding: fictrac_control::RecordingEncoding::Configured,
            startup_fallback: None,
            fallback_notice: String::new(),
            temporary_config: false,
            paused: false,
            realtime: true,
            tool: Tool::Inspect,
            edit_existing: false,
            calibration_review: true,
            calibration_redraw: false,
            calibration_previous_paused: None,
            magnifier: true,
            points: vec![],
            square_slots: None,
            window_rect: None,
            window_inner_size: None,
            framebuffer_limit: 4096,
            max_window_inner_size: egui::vec2(4096.0, 4096.0),
            requested_window_size: None,
            calibration_preview_size: None,
            last_title: String::new(),
            plane: "xy".into(),
            full_image: false,
            changed: None,
            revision: 0,
            textures: vec![],
            texture_frame: u64::MAX,
            calibration_cache: std::cell::RefCell::new(None),
            texture_image: None,
            spawn_request: 0,
            closed: false,
            native_window: true,
            close_pending: false,
            last_disk_check: Instant::now(),
            status: String::new(),
            auto_apply: true,
            pending_drag: None,
            recording_path: crate::runtime::recording_root()
                .join(format!("VR{id}"))
                .to_string_lossy()
                .into_owned(),
        }
    }
    pub fn use_fallback(&mut self, reason: &str) {
        if let Some(doc) = self.startup_fallback.take() {
            self.history = DocumentHistory::new(doc.text().to_owned());
            self.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
            self.temporary_config = true;
            self.saved = self.history.text.clone();
            self.fallback_notice = format!("Camera unavailable ({reason}); playing sample video");
            let result = self.start();
            self.report(result);
        }
    }
    pub fn camera_format(
        &mut self,
        request: fictrac_camera::CameraFormatRequest,
    ) -> Result<(), String> {
        request.validate().map_err(|e| e.to_string())?;
        if self.source_edit.parse::<u32>().is_err() {
            return Err("Camera format requires a live camera source".into());
        }
        if let Some(caps) = &self.snapshot.camera {
            let supported = caps.formats.iter().any(|f| {
                f.width == request.width
                    && f.height == request.height
                    && request.fps.is_none_or(|fps| {
                        f.frame_rates
                            .iter()
                            .any(|r| fps >= r.min_fps - 1e-6 && fps <= r.max_fps + 1e-6)
                    })
            });
            if !supported {
                return Err("Requested dimensions/rate are not advertised by this camera; refresh camera controls".into());
            }
        }
        let mut doc = self.document()?;
        if doc.get("rust_calibration_size").is_some() {
            doc = fictrac_io::rescale_calibration_reference(&doc, request.width, request.height)
                .map_err(|e| e.to_string())?;
        }
        doc.set("rust_camera_width", &request.width.to_string())
            .map_err(|e| e.to_string())?;
        doc.set("rust_camera_height", &request.height.to_string())
            .map_err(|e| e.to_string())?;
        doc.set("src_fps", &request.fps.unwrap_or(-1.0).to_string())
            .map_err(|e| e.to_string())?;
        self.edit(doc.text().to_owned());
        self.start()
    }
    pub fn refresh_cameras(&mut self) {
        match fictrac_camera::list_cameras() {
            Ok(cameras) => {
                self.cameras = cameras;
                self.camera_error.clear();
            }
            Err(error) => self.camera_error = error.to_string(),
        }
    }
    pub fn set_display_fps(&mut self, fps: u32) -> Result<(), String> {
        if !(1..=60).contains(&fps) {
            return Err("Display FPS must be an integer between 1 and 60".into());
        }
        if let Some(worker) = &self.worker {
            worker.send(Command::DisplayFps(fps))?;
        }
        self.display_fps = fps;
        Ok(())
    }
    pub fn begin_recording(&mut self) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = PathBuf::from(&self.recording_path).join(format!("take-{stamp}"));
        self.send(Command::RecordOptions(
            path,
            fictrac_control::RecordOptions {
                raw: self.record_raw,
                debug: self.record_debug,
            },
            fictrac_control::MediaRecordingTuning {
                encoding: self.recording_encoding,
                ..fictrac_control::MediaRecordingTuning::default()
            },
        ));
    }
    pub fn window_title(&self) -> String {
        self.window_title_for_width(self.window_rect.map(|r| r[2]).unwrap_or(520.0))
    }
    fn window_title_for_width(&self, width: f32) -> String {
        let source = if self.snapshot.source_label.is_empty() {
            if self.running_source.is_empty() {
                self.source_edit.clone()
            } else {
                self.running_source.clone()
            }
        } else {
            self.snapshot.source_label.clone()
        };
        let input = if self.running_source.is_empty() {
            &self.source_edit
        } else {
            &self.running_source
        };
        let short_source = short_source_label(input, &source);
        // Show applied transport state. A rejected draft edit stays visible in
        // the inspector, but must not relabel the old running output ports.
        let (udp, zmq) = if let Some(outputs) = &self.snapshot.outputs {
            let udp = outputs
                .udp_destination
                .as_deref()
                .filter(|_| outputs.udp_enabled)
                .and_then(|destination| destination.parse::<std::net::SocketAddr>().ok())
                .map(|destination| destination.port().to_string())
                .unwrap_or_else(|| "off".into());
            let zmq = outputs
                .zmq
                .as_ref()
                .map(|publisher| {
                    let port = publisher.endpoint.rsplit(':').next().unwrap_or("?");
                    if publisher.worker_alive && publisher.failures == 0 {
                        port.to_owned()
                    } else {
                        format!("{port} failed")
                    }
                })
                .unwrap_or_else(|| "off".into());
            (udp, zmq)
        } else {
            self.document()
                .map(|doc| {
                    let port = |key| {
                        doc.get(key)
                            .filter(|value| !matches!(*value, "-1" | "0"))
                            .unwrap_or("off")
                    };
                    (port("sock_port").to_owned(), port("zmq_port").to_owned())
                })
                .unwrap_or_default()
        };
        if width >= 900.0 {
            format!(
                "{} · ZMQ {zmq} · {short_source} · UDP {udp} · HejjéTrac · {source}",
                self.display_name()
            )
        } else if width >= 650.0 {
            format!(
                "{} · ZMQ {zmq} · {short_source} · UDP {udp} · HejjéTrac",
                self.display_name()
            )
        } else {
            let output = if zmq != "off" {
                format!("ZMQ {zmq}")
            } else if udp != "off" {
                format!("UDP {udp}")
            } else {
                "Outputs off".into()
            };
            format!("{} · {output} · {short_source}", self.display_name())
        }
    }
    pub fn display_name(&self) -> String {
        self.document()
            .ok()
            .and_then(|d| {
                d.get("rust_instance_name")
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.chars().take(64).collect())
            })
            .or_else(|| {
                let path = self.path.as_ref().or(self.source_base.as_ref())?;
                // Original FicTrac rigs usually keep a plain config.txt inside
                // VR3/ (or BR3/), without a Rust-specific instance name.
                [path.file_stem(), path.parent().and_then(|p| p.file_name())]
                    .into_iter()
                    .flatten()
                    .filter_map(|name| name.to_str())
                    .find(|name| {
                        name.strip_prefix("VR")
                            .or_else(|| name.strip_prefix("BR"))
                            .is_some_and(|suffix| {
                                !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
                            })
                    })
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("VR{}", self.id))
    }
    pub fn document(&self) -> Result<ConfigDocument, String> {
        ConfigDocument::parse(self.history.text.clone()).map_err(|e| e.to_string())
    }
    pub fn edit(&mut self, text: String) {
        self.history.replace(text);
        self.changed = Some(Instant::now());
    }
    pub fn fields(&mut self, values: &[(&str, String)]) -> Result<(), String> {
        let mut doc = self.document()?;
        for (key, value) in values {
            doc.set(key, value).map_err(|e| e.to_string())?;
        }
        if doc.get("rust_calibration_size").is_some()
            && values.iter().any(|(key, _)| {
                matches!(
                    *key,
                    "roi_circ" | "roi_ignr" | "c2a_cnrs_xy" | "c2a_cnrs_yz" | "c2a_cnrs_xz"
                )
            })
        {
            let [width, height]: [usize; 2] = doc
                .vector("rust_calibration_size")
                .map_err(|e| e.to_string())?;
            doc = fictrac_io::capture_calibration_reference(&doc, width, height)
                .map_err(|e| e.to_string())?;
        }
        self.edit(doc.text().to_owned());
        self.source_edit = doc.get("src_fn").unwrap_or("").to_owned();
        Ok(())
    }
    pub fn start(&mut self) -> Result<(), String> {
        let doc = self.document()?;
        self.ensure_source_available(&doc)?;
        let source = doc
            .source(self.path.as_deref().or(self.source_base.as_deref()))
            .map_err(|e| e.to_string())?;
        if matches!(source, SourceSpec::Camera(_)) {
            fictrac_camera::request_permission().map_err(|e| e.to_string())?;
        }
        self.worker = None;
        self.camera_edit = None;
        self.snapshot = Snapshot::default();
        self.revision = 0;
        self.textures.clear();
        self.texture_image = None;
        self.texture_frame = u64::MAX;
        self.running_source = doc.get("src_fn").unwrap_or("").into();
        self.running_fps = doc.number("src_fps", -1.0);
        self.worker = Some(Worker::start_at(
            source,
            doc,
            self.realtime,
            self.path.clone().or(self.source_base.clone()),
        )?);
        if let Some(worker) = &self.worker {
            worker.send(Command::DisplayFps(self.display_fps))?;
        }
        self.paused = false;
        self.changed = None;
        self.error.clear();
        Ok(())
    }
    pub fn ensure_source_available(&self, doc: &ConfigDocument) -> Result<(), String> {
        if let Some(id) = doc
            .get("src_fn")
            .and_then(|source| source.parse::<u32>().ok())
            && let Some(owner) = self.occupied_cameras.get(&id)
        {
            return Err(format!(
                "Camera {id} is already used by {owner}. Close that rig or choose a free camera."
            ));
        }
        Ok(())
    }
    pub fn select_camera(&mut self, id: u32) -> Result<(), String> {
        let mut doc = self.document()?;
        doc.set("src_fn", &id.to_string())
            .map_err(|e| e.to_string())?;
        self.ensure_source_available(&doc)?;
        if self.running_source == id.to_string() && self.worker.is_some() && !self.snapshot.ended {
            if doc.text() != self.history.text {
                // Selecting the running camera can also repair a rejected
                // source draft without needlessly closing that capture stream.
                self.edit(doc.text().to_owned());
                self.source_edit = id.to_string();
                self.apply()?;
            }
            return Ok(());
        }
        self.edit(doc.text().to_owned());
        self.source_edit = id.to_string();
        self.start()
    }
    pub fn apply(&mut self) -> Result<(), String> {
        let doc = self.document()?;
        let source = doc.get("src_fn").unwrap_or("");
        if self.worker.is_some()
            && (source != self.running_source
                || (source.parse::<u32>().is_ok()
                    && doc.number("src_fps", -1.0) != self.running_fps))
        {
            return self.start();
        }
        self.revision += 1;
        if let Some(worker) = &self.worker {
            worker.send(Command::Configure(doc, self.revision))?;
        }
        self.changed = None;
        Ok(())
    }
    pub fn load(&mut self, path: PathBuf) -> Result<(), String> {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let doc = ConfigDocument::parse(text).map_err(|e| e.to_string())?;
        self.worker = None;
        self.record_raw = doc.boolean("save_raw");
        self.record_debug = doc.boolean("save_debug");
        self.recording_encoding = fictrac_control::RecordingEncoding::Configured;
        self.history = DocumentHistory::new(doc.text().to_owned());
        self.zmq_resume_port = None;
        self.saved = doc.text().to_owned();
        self.path = Some(path);
        self.disk_conflict = false;
        self.source_edit = doc.get("src_fn").unwrap_or("").into();
        self.points.clear();
        self.start()
    }
    pub fn video(&mut self, path: PathBuf) -> Result<(), String> {
        let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
        self.fields(&[("src_fn", path.to_string_lossy().into_owned())])?;
        self.start()
    }
    pub fn save(&mut self, as_new: bool) -> Result<(), String> {
        let mut doc = self.document()?;
        let path = if as_new || self.path.is_none() || self.temporary_config {
            rfd::FileDialog::new()
                .add_filter("FicTrac config", &["txt"])
                .set_file_name("config.txt")
                .save_file()
                .ok_or("Save cancelled")?
        } else {
            self.path.clone().ok_or("No config path")?
        };
        if self.path.as_ref() != Some(&path)
            && let Ok(fictrac_io::SourceSpec::Video(source)) =
                doc.source(self.path.as_deref().or(self.source_base.as_deref()))
            && let Ok(absolute) = fs::canonicalize(source)
        {
            // Keep Save As attached to the same video after moving the config directory.
            doc.set("src_fn", &absolute.to_string_lossy())
                .map_err(|e| e.to_string())?;
        }
        if !as_new
            && path.is_file()
            && fs::read_to_string(&path).map_err(|e| e.to_string())? != self.saved
        {
            return Err(
                "File changed on disk. Reload or Save As; the external edit was preserved.".into(),
            );
        }
        let tmp = path.with_extension(format!("fictrac-{}.tmp", std::process::id()));
        fs::write(&tmp, doc.text())
            .and_then(|_| fs::rename(&tmp, &path))
            .map_err(|e| {
                format!(
                    "Cannot save {}: {e}. Use Save As to choose a writable configuration folder.",
                    path.display()
                )
            })?;
        self.history.replace(doc.text().to_owned());
        self.source_edit = doc.get("src_fn").unwrap_or("").into();
        self.saved = doc.text().to_owned();
        self.path = Some(path.clone());
        self.source_base = Some(path);
        self.disk_conflict = false;
        self.temporary_config = false;
        self.status = "Configuration saved".into();
        Ok(())
    }
    fn observe_worker_snapshot(&mut self, snapshot: Snapshot) {
        // A new applied revision acknowledges recovery from an earlier rejected
        // configuration. Clear only that exact error, preserving unrelated local
        // errors and any current command failure.
        let recovered_configuration = snapshot.command_error.is_none()
            && snapshot.revision > self.snapshot.revision
            && self
                .snapshot
                .command_error
                .as_ref()
                .is_some_and(|previous| self.error == *previous);
        if let Some(error) = &snapshot.command_error {
            self.error = error.clone();
        } else if recovered_configuration {
            self.error.clear();
        }
        self.snapshot = snapshot;
    }
    pub fn update(&mut self) {
        if let Some(worker) = &self.worker {
            let snapshot = worker.latest();
            self.observe_worker_snapshot(snapshot);
        }
        if self.snapshot.ended && self.snapshot.frames == 0 && self.startup_fallback.is_some() {
            let reason = self.snapshot.notice.clone();
            self.use_fallback(&reason);
        } else if self.snapshot.frames > 0 {
            self.startup_fallback = None;
        }
        if self.auto_apply && self.changed.is_some_and(|t| t.elapsed().as_millis() > 400) {
            let r = self.apply();
            self.report(r);
        }
        if !self.temporary_config && self.last_disk_check.elapsed().as_millis() > 600 {
            self.last_disk_check = Instant::now();
            if let Some(path) = &self.path
                && let Ok(text) = fs::read_to_string(path)
                && text != self.saved
            {
                if self.history.text == self.saved {
                    match ConfigDocument::parse(text.clone()) {
                        Ok(doc) => {
                            self.history.replace(text.clone());
                            self.saved = text;
                            self.source_edit = doc.get("src_fn").unwrap_or("").into();
                            self.changed = Some(Instant::now());
                            self.status = "Reloaded an external file edit".into();
                        }
                        Err(e) => {
                            self.error = format!("External config is invalid: {e}");
                        }
                    }
                } else {
                    self.disk_conflict = true;
                }
            }
        }
    }
    pub fn report(&mut self, result: Result<(), String>) {
        if let Err(e) = result {
            self.error = e;
        } else {
            self.error.clear();
        }
    }
    pub fn send(&mut self, command: Command) {
        if let Some(worker) = &self.worker {
            let r = worker.send(command);
            self.report(r);
        }
    }
}

fn short_source_label(input: &str, actual: &str) -> String {
    if let Ok(id) = input.parse::<u32>() {
        // Use an actual V4L path when the backend provided one; never infer a
        // device path from an enumeration ordinal on a different platform.
        if let Some(start) = actual.find("/dev/video") {
            let path = &actual[start..];
            let end = path
                .find(|c: char| c.is_whitespace() || c == ')')
                .unwrap_or(path.len());
            return path[..end].to_owned();
        }
        return format!("Camera {id}");
    }
    actual
        .rsplit(['/', '\\'])
        .find(|s| !s.is_empty())
        .unwrap_or(actual)
        .to_owned()
}

pub(crate) struct CalibrationCache {
    pub text: String,
    pub width: usize,
    pub height: usize,
    pub config: Result<fictrac_core::Config, String>,
    pub crop: Option<egui::Rect>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn successful_configuration_ack_clears_only_the_superseded_command_error()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = ConfigDocument::parse("src_fn:sample.mp4\n".into())?;
        let mut session = Session::new(1, document, None);
        let rejected = Snapshot {
            revision: 2,
            command_error: Some("Configuration rejected: occupied publisher port".into()),
            ..Snapshot::default()
        };
        session.observe_worker_snapshot(rejected.clone());
        assert_eq!(
            session.error,
            rejected.command_error.as_deref().unwrap_or("")
        );
        let applied = Snapshot {
            revision: 4,
            notice: "Configuration applied; tracking epoch reset".into(),
            ..Snapshot::default()
        };
        session.observe_worker_snapshot(applied.clone());
        assert!(session.error.is_empty());

        session.observe_worker_snapshot(rejected.clone());
        session.error = "Cannot save configuration: folder is read-only".into();
        session.observe_worker_snapshot(applied.clone());
        assert_eq!(
            session.error,
            "Cannot save configuration: folder is read-only"
        );

        session.observe_worker_snapshot(rejected.clone());
        session.observe_worker_snapshot(Snapshot {
            revision: 2,
            ..Snapshot::default()
        });
        assert!(
            !session.error.is_empty(),
            "No successful revision was acknowledged"
        );
        session.observe_worker_snapshot(rejected);
        session.observe_worker_snapshot(Snapshot {
            revision: 4,
            command_error: Some("Configuration rejected: publisher unavailable".into()),
            ..Snapshot::default()
        });
        assert_eq!(
            session.error,
            "Configuration rejected: publisher unavailable"
        );
        Ok(())
    }

    #[test]
    fn real_publisher_retry_clears_the_rejected_port_banner_after_apply()
    -> Result<(), Box<dyn std::error::Error>> {
        fn wait_for(
            session: &mut Session,
            predicate: impl Fn(&Session) -> bool,
        ) -> Result<(), String> {
            let deadline = Instant::now() + std::time::Duration::from_secs(15);
            loop {
                session.update();
                if predicate(session) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Session response timed out: revision={}, requested={}, command_error={:?}, error={}, notice={}",
                        session.snapshot.revision,
                        session.revision,
                        session.snapshot.command_error,
                        session.error,
                        session.snapshot.notice
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut document = ConfigDocument::parse(fs::read_to_string(
            root.join("examples/upstream-config.txt"),
        )?)?;
        document.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )?;
        document.set("zmq_port", "-1")?;
        document.set("sock_port", "-1")?;
        document.set("save_raw", "n")?;
        document.set("save_debug", "n")?;
        let mut session = Session::new(1, document, None);
        session.auto_apply = false;
        session.start()?;
        wait_for(&mut session, |s| s.snapshot.frames >= 3)?;
        session.send(Command::Pause(true));
        wait_for(&mut session, |s| s.snapshot.paused)?;
        let source_frames = session.snapshot.frames;
        let busy = std::net::TcpListener::bind("127.0.0.1:0")?;
        session.fields(&[("zmq_port", busy.local_addr()?.port().to_string())])?;
        session.apply()?;
        wait_for(&mut session, |s| s.snapshot.command_error.is_some())?;
        assert!(session.error.contains("Configuration rejected"));
        assert!(session.error.contains("rollback succeeded"));
        assert_eq!(session.snapshot.frames, source_frames);

        let free = std::net::TcpListener::bind("127.0.0.1:0")?;
        let free_port = free.local_addr()?.port();
        drop(free);
        session.fields(&[("zmq_port", free_port.to_string())])?;
        session.apply()?;
        assert!(
            !session.error.is_empty(),
            "A queued request is not an acknowledgement"
        );
        wait_for(&mut session, |s| {
            s.snapshot.revision == s.revision && s.snapshot.command_error.is_none()
        })?;
        assert!(
            session.error.is_empty(),
            "The rejected-port banner survived a successful apply"
        );
        assert_eq!(session.snapshot.frames, source_frames);
        let publisher = session
            .snapshot
            .outputs
            .as_ref()
            .and_then(|outputs| outputs.zmq.as_ref())
            .ok_or("Publisher status missing after successful apply")?;
        assert_eq!(publisher.endpoint, format!("tcp://127.0.0.1:{free_port}"));
        assert!(publisher.worker_alive);
        Ok(())
    }
    #[test]
    fn title_uses_applied_ports_and_source_instead_of_rejected_draft()
    -> Result<(), Box<dyn std::error::Error>> {
        let document = ConfigDocument::parse(
            "rust_instance_name:VR7\nsrc_fn:/draft/input.mp4\nsock_port:2222\nzmq_port:9999\n"
                .into(),
        )?;
        let mut session = Session::new(7, document, None);
        session.snapshot.source_label = "/actual/input.mp4".into();
        session.snapshot.outputs = Some(fictrac_io::OutputStatus {
            udp_enabled: true,
            udp_destination: Some("127.0.0.1:1111".into()),
            serial_enabled: false,
            zmq_payload: None,
            zmq: Some(fictrac_io::ZmqOutputStatus {
                endpoint: "tcp://127.0.0.1:9871".into(),
                queue_capacity: 256,
                queue_len: 0,
                enqueued: 0,
                published: 0,
                dropped: 0,
                failures: 0,
                worker_alive: true,
                last_error: None,
            }),
        });
        let title = session.window_title();
        assert!(title.contains("VR7"));
        assert_eq!(title, "VR7 · ZMQ 9871 · input.mp4");
        assert!(
            session
                .window_title_for_width(960.0)
                .contains("/actual/input.mp4")
        );
        assert!(session.window_title_for_width(700.0).contains("UDP 1111"));
        assert!(!title.contains("2222") && !title.contains("9999"));
        if let Some(publisher) = session
            .snapshot
            .outputs
            .as_mut()
            .and_then(|outputs| outputs.zmq.as_mut())
        {
            publisher.worker_alive = false;
            publisher.failures = 1;
        }
        assert!(session.window_title().contains("ZMQ 9871 failed"));
        session.snapshot.outputs = None;
        assert!(session.window_title().contains("ZMQ 9999"));
        assert!(session.window_title_for_width(700.0).contains("UDP 2222"));
        Ok(())
    }
    #[test]
    fn camera_titles_are_short_and_never_invent_device_paths() {
        assert_eq!(
            short_source_label("2", "Huge camera name (AVFoundation: 0x00000001)"),
            "Camera 2"
        );
        assert_eq!(
            short_source_label("7", "External camera (/dev/video7)"),
            "/dev/video7"
        );
        assert_eq!(
            short_source_label("C:\\Videos\\trial.mp4", "C:\\Videos\\trial.mp4"),
            "trial.mp4"
        );
    }
    #[test]
    fn loading_another_config_refreshes_record_defaults_and_keeps_manual_edits_local()
    -> Result<(), String> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = ConfigDocument::parse(
            fs::read_to_string(root.join("examples/upstream-config.txt"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        doc.set(
            "src_fn",
            &root.join("examples/sample.mp4").to_string_lossy(),
        )
        .map_err(|error| error.to_string())?;
        doc.set("save_raw", "y")
            .map_err(|error| error.to_string())?;
        doc.set("save_debug", "y")
            .map_err(|error| error.to_string())?;
        let mut session = Session::new(1, doc.clone(), None);
        assert!(session.record_raw && session.record_debug);
        for key in ["save_raw", "save_debug"] {
            doc.set(key, "n").map_err(|error| error.to_string())?;
        }
        doc.set("sock_port", "0")
            .map_err(|error| error.to_string())?;
        doc.set("zmq_port", "0")
            .map_err(|error| error.to_string())?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("hejjetrac-load-record-defaults-{stamp}"));
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let path = directory.join("config.txt");
        fs::write(&path, doc.text()).map_err(|error| error.to_string())?;
        session.load(path)?;
        assert!(!session.record_raw && !session.record_debug);
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        while session.snapshot.frames < 2 {
            session.update();
            if Instant::now() >= deadline {
                return Err("loaded real-video source did not advance".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // Ordinary numerical edits preserve the user's transient recording
        // selection; opening a different config is the reset boundary.
        session.record_raw = true;
        session.fields(&[("opt_tol", "0.002".into())])?;
        session.apply()?;
        assert!(session.record_raw && !session.record_debug);
        assert_eq!(session.document()?.get("save_raw"), Some("n"));
        session.worker = None;
        fs::remove_dir_all(directory).map_err(|error| error.to_string())?;
        Ok(())
    }
    #[test]
    fn record_selection_starts_from_the_loaded_config_flags() -> Result<(), String> {
        for raw in [false, true] {
            for debug in [false, true] {
                let doc = ConfigDocument::parse(format!(
                    "src_fn:4\nsave_raw:{}\nsave_debug:{}\n",
                    if raw { "y" } else { "n" },
                    if debug { "y" } else { "n" },
                ))
                .map_err(|error| error.to_string())?;
                let session = Session::new(1, doc, None);
                assert_eq!(session.record_raw, raw);
                assert_eq!(session.record_debug, debug);
            }
        }
        Ok(())
    }
    #[test]
    fn repeated_camera_selection_and_busy_peer_keep_the_existing_worker() -> Result<(), String> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut doc = ConfigDocument::parse(
            fs::read_to_string(root.join("examples/upstream-config.txt"))
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        doc.set("src_fn", "4").map_err(|e| e.to_string())?;
        let mut session = Session::new(1, doc.clone(), None);
        // Exercise the real decoding/tracking worker while representing its
        // source ownership as a camera; this test never opens a hardware device.
        session.running_source = "4".into();
        session.worker = Some(Worker::start(
            SourceSpec::Video(root.join("examples/sample.mp4")),
            doc,
            true,
        )?);
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        while session.snapshot.frames < 3 {
            session.update();
            if Instant::now() >= deadline {
                return Err("real source worker did not deliver frames".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let shared = session
            .worker
            .as_ref()
            .ok_or("missing running worker")?
            .snapshot
            .clone();
        let original = session.history.text.clone();
        session.select_camera(4)?;
        assert_eq!(session.history.text, original);
        assert_eq!(session.revision, 0);
        session.occupied_cameras.insert(2, "VR2 (window 2)".into());
        assert!(session.select_camera(2).is_err());
        assert_eq!(session.history.text, original);
        assert!(session.snapshot.image.is_some());
        session.fields(&[("src_fn", "2".into())])?;
        assert!(session.start().is_err());
        session.select_camera(4)?;
        assert_eq!(session.document()?.get("src_fn"), Some("4"));
        assert_eq!(session.source_edit, "4");
        assert!(std::sync::Arc::ptr_eq(
            &shared,
            &session.worker.as_ref().ok_or("worker was lost")?.snapshot,
        ));
        Ok(())
    }
    #[test]
    fn compact_title_prioritizes_the_active_udp_output() -> Result<(), String> {
        let doc = ConfigDocument::parse("src_fn:4\nsock_port:1113\nzmq_port:-1\n".into())
            .map_err(|e| e.to_string())?;
        let mut session = Session::new(3, doc, None);
        assert_eq!(session.window_title(), "VR3 · UDP 1113 · Camera 4");
        session.snapshot.outputs = Some(fictrac_io::OutputStatus {
            udp_enabled: true,
            udp_destination: Some("127.0.0.1:1114".into()),
            serial_enabled: false,
            zmq_payload: None,
            zmq: None,
        });
        assert_eq!(session.window_title(), "VR3 · UDP 1114 · Camera 4");
        if let Some(outputs) = &mut session.snapshot.outputs {
            outputs.udp_enabled = false;
        }
        assert_eq!(session.window_title(), "VR3 · Outputs off · Camera 4");
        Ok(())
    }
    #[test]
    fn legacy_rig_names_come_from_the_selected_config() -> Result<(), String> {
        for (path, expected) in [
            ("/rigs/VR3/config.txt", "VR3"),
            ("/rigs/BR4/config.txt", "BR4"),
            ("/rigs/VR2.txt", "VR2"),
            ("/rigs/VR/config.txt", "VR1"),
            ("/rigs/VR3draft/config.txt", "VR1"),
        ] {
            let doc = ConfigDocument::parse("src_fn:4\n".into()).map_err(|e| e.to_string())?;
            let mut session = Session::new(1, doc, Some(PathBuf::from(path)));
            assert_eq!(session.display_name(), expected);
            assert!(session.window_title().starts_with(expected));
            session.fields(&[("rust_instance_name", "Rig A".into())])?;
            assert_eq!(session.display_name(), "Rig A");
        }
        Ok(())
    }
}
