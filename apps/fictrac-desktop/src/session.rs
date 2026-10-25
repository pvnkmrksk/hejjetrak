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
    pub show_diagnostics: bool,
    pub show_overlays: bool,
    pub cameras: Vec<fictrac_camera::CameraInfo>,
    pub camera_edit: Option<fictrac_camera::CameraFormatRequest>,
    pub camera_error: String,
    pub record_raw: bool,
    pub record_debug: bool,
    pub startup_fallback: Option<ConfigDocument>,
    pub fallback_notice: String,
    pub temporary_config: bool,
    pub paused: bool,
    pub realtime: bool,
    pub tool: Tool,
    pub edit_existing: bool,
    pub magnifier: bool,
    pub points: Vec<[f64; 2]>,
    pub square_slots: Option<[Option<[f64; 2]>; 4]>,
    pub window_rect: Option<[f32; 4]>,
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
            show_diagnostics: false,
            show_overlays: false,
            cameras: fictrac_camera::list_cameras().unwrap_or_default(),
            camera_edit: None,
            camera_error: String::new(),
            record_raw: true,
            record_debug: false,
            startup_fallback: None,
            fallback_notice: String::new(),
            temporary_config: false,
            paused: false,
            realtime: true,
            tool: Tool::Inspect,
            edit_existing: false,
            magnifier: true,
            points: vec![],
            square_slots: None,
            window_rect: None,
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
        ));
    }
    pub fn window_title(&self) -> String {
        let source = if self.snapshot.source_label.is_empty() {
            self.source_edit.clone()
        } else {
            self.snapshot.source_label.clone()
        };
        let camera_name = if self.snapshot.source_label.is_empty() {
            self.running_source
                .parse::<u32>()
                .ok()
                .and_then(|id| self.cameras.iter().find(|camera| camera.id == id))
                .map(|c| format!(" · {}", c.name))
                .unwrap_or_default()
        } else {
            String::new()
        };
        // Show applied transport state. A rejected draft edit stays visible in
        // the inspector, but must not relabel the old running output ports.
        let ports = if let Some(outputs) = &self.snapshot.outputs {
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
            format!("UDP {udp} · ZMQ {zmq}")
        } else {
            self.document()
                .map(|doc| {
                    let port = |key| {
                        doc.get(key)
                            .filter(|value| !matches!(*value, "-1" | "0"))
                            .unwrap_or("off")
                    };
                    format!("UDP {} · ZMQ {}", port("sock_port"), port("zmq_port"))
                })
                .unwrap_or_default()
        };
        format!(
            "{} · {}{} · {} · hejjetrak",
            self.display_name(),
            source,
            camera_name,
            ports
        )
    }
    pub fn display_name(&self) -> String {
        self.document()
            .ok()
            .and_then(|d| {
                d.get("rust_instance_name")
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.chars().take(64).collect())
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
        self.paused = false;
        self.changed = None;
        self.error.clear();
        Ok(())
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
        self.history = DocumentHistory::new(doc.text().to_owned());
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
            .map_err(|e| e.to_string())?;
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
    pub fn update(&mut self) {
        if let Some(worker) = &self.worker {
            self.snapshot = worker.latest();
            if let Some(error) = &self.snapshot.command_error {
                self.error = error.clone();
            }
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
        assert!(title.contains("/actual/input.mp4"));
        assert!(title.contains("UDP 1111 · ZMQ 9871"));
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
        assert!(session.window_title().contains("UDP 2222 · ZMQ 9999"));
        Ok(())
    }
}
