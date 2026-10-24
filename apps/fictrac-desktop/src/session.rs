use crate::runtime::{Command, Snapshot, Worker};
use eframe::egui;
use fictrac_io::{ConfigDocument, DocumentHistory, SourceSpec};
use std::{fs, path::PathBuf, time::Instant};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Inspect,
    Rim,
    Exclude,
    Forward,
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
    pub raw_tab: bool,
    pub paused: bool,
    pub realtime: bool,
    pub tool: Tool,
    pub points: Vec<[f64; 2]>,
    pub plane: String,
    pub full_image: bool,
    pub changed: Option<Instant>,
    pub revision: u64,
    pub textures: Vec<egui::TextureHandle>,
    pub texture_frame: u64,
    pub spawn_request: usize,
    pub closed: bool,
    pub native_window: bool,
    pub close_pending: bool,
    pub last_disk_check: Instant,
    pub status: String,
    pub auto_apply: bool,
    pub pending_drag: Option<(usize, usize)>,
    pub drag_start: Option<[f64; 2]>,
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
            raw_tab: false,
            paused: false,
            realtime: true,
            tool: Tool::Inspect,
            points: vec![],
            plane: "xy".into(),
            full_image: false,
            changed: None,
            revision: 0,
            textures: vec![],
            texture_frame: u64::MAX,
            spawn_request: 0,
            closed: false,
            native_window: true,
            close_pending: false,
            last_disk_check: Instant::now(),
            status: String::new(),
            auto_apply: true,
            pending_drag: None,
            drag_start: None,
            recording_path: format!("runs/gui-{id}"),
        }
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
        self.snapshot = Snapshot::default();
        self.revision = 0;
        self.textures.clear();
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
        let path = if as_new || self.path.is_none() {
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
        self.status = "Configuration saved".into();
        Ok(())
    }
    pub fn update(&mut self) {
        if let Some(worker) = &self.worker {
            self.snapshot = worker.latest();
        }
        if self.auto_apply && self.changed.is_some_and(|t| t.elapsed().as_millis() > 400) {
            let r = self.apply();
            self.report(r);
        }
        if self.last_disk_check.elapsed().as_millis() > 600 {
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
