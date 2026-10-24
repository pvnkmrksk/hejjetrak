//! Lossless FicTrac configuration editing. Unknown fields and comments are data.
use crate::{Error, Result};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceSpec {
    Camera(u32),
    Video(PathBuf),
}
#[derive(Clone, Debug)]
pub struct ConfigDocument {
    text: String,
    values: BTreeMap<String, String>,
}
impl ConfigDocument {
    pub fn parse(text: String) -> Result<Self> {
        if text.len() > 1_000_000 {
            return Err(Error::Config("config exceeds 1 MB".into()));
        }
        let values = crate::config::legacy(&text)?;
        Ok(Self { text, values })
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    pub fn fields(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
    pub fn number(&self, key: &str, default: f64) -> f64 {
        self.get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }
    pub fn boolean(&self, key: &str) -> bool {
        self.get(key)
            .is_some_and(|v| matches!(v, "y" | "true" | "1"))
    }
    pub fn vector<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<T> {
        let value = self
            .get(key)
            .ok_or_else(|| Error::Config(format!("missing {key}")))?;
        Ok(serde_json::from_str(
            &value.replace('{', "[").replace('}', "]"),
        )?)
    }
    /// Apply a field edit without normalizing unrelated lines, whitespace or comments.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        if key.trim().is_empty()
            || key.contains(['\n', '\r', ':', '#'])
            || value.contains(['\n', '\r', '#'])
        {
            return Err(Error::Config(
                "parameter names/values must fit on one unambiguous line".into(),
            ));
        }
        if self.get(key) == Some(value.trim()) {
            return Ok(());
        }
        let mut found = false;
        let mut output = String::new();
        for line in self.text.split_inclusive('\n') {
            let code = line.split('#').next().unwrap_or("");
            if let Some((left, right)) = code.split_once(':')
                && left.trim() == key
            {
                found = true;
                let prefix = code.len() - right.len();
                let leading = right.len() - right.trim_start_matches([' ', '\t']).len();
                let trailing = right.len() - right.trim_end().len();
                let suffix_start = code.len().saturating_sub(trailing).max(prefix + leading);
                output.push_str(&line[..prefix + leading]);
                output.push_str(value.trim());
                output.push_str(&line[suffix_start..]);
            } else {
                output.push_str(line);
            }
        }
        if !found {
            let newline = if self.text.contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            if !output.is_empty() && !output.ends_with('\n') {
                output.push_str(newline);
            }
            output.push_str(&format!("{key:<20} : {}{newline}", value.trim()));
        }
        *self = Self::parse(output)?;
        Ok(())
    }
    pub fn source(&self, config_path: Option<&Path>) -> Result<SourceSpec> {
        let value = self
            .get("src_fn")
            .ok_or_else(|| Error::Config("missing src_fn".into()))?;
        if let Ok(id) = value.parse::<u32>() {
            return Ok(SourceSpec::Camera(id));
        }
        if value.is_empty() {
            return Err(Error::Config("src_fn is empty".into()));
        }
        let path = PathBuf::from(value.trim_matches('"'));
        // Preserve legacy CWD precedence, and allow self-contained config/sample directories.
        if path.is_absolute() || path.exists() {
            return Ok(SourceSpec::Video(path));
        }
        Ok(SourceSpec::Video(
            config_path
                .and_then(Path::parent)
                .unwrap_or(Path::new("."))
                .join(path),
        ))
    }
    /// Find the next BR/VR sibling; otherwise copy while incrementing only a numeric camera source.
    pub fn next_instance(&self, path: Option<&Path>) -> Result<(Self, Option<PathBuf>)> {
        if let Some(path) = path
            && let Some(parent) = path.parent()
        {
            let candidates = if parent
                .file_name()
                .and_then(|p| p.to_str())
                .is_some_and(|p| p.starts_with("VR") || p.starts_with("BR"))
            {
                let name = parent.file_name().and_then(|p| p.to_str()).unwrap_or("");
                let index = name.get(2..).and_then(|p| p.parse::<u32>().ok());
                index
                    .and_then(|i| i.checked_add(1))
                    .map(|next_index| {
                        vec![
                            parent
                                .with_file_name(format!("{}{}", &name[..2], next_index))
                                .join(path.file_name().unwrap_or_default()),
                        ]
                    })
                    .unwrap_or_default()
            } else {
                let stem = path.file_stem().and_then(|p| p.to_str()).unwrap_or("");
                let split = stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
                stem.get(split..)
                    .and_then(|s| s.parse::<u32>().ok())
                    .and_then(|n| n.checked_add(1))
                    .map(|n| {
                        vec![path.with_file_name(format!(
                            "{}{}.{}",
                            &stem[..split],
                            n,
                            path.extension().and_then(|x| x.to_str()).unwrap_or("txt")
                        ))]
                    })
                    .unwrap_or_default()
            };
            for next in candidates {
                if next.is_file() {
                    return Ok((Self::parse(std::fs::read_to_string(&next)?)?, Some(next)));
                }
            }
        }
        let mut next = self.clone();
        if let Some(value) = self.get("src_fn")
            && let Ok(id) = value.parse::<u32>()
        {
            let id = id
                .checked_add(1)
                .ok_or_else(|| Error::Config("camera ID overflow".into()))?;
            next.set("src_fn", &id.to_string())?;
        }
        for key in ["sock_port", "zmq_port"] {
            if let Some(value) = self.get(key) {
                let port = value.parse::<i32>().map_err(|_| {
                    Error::Config(format!(
                        "{key} must be an integer before copying this instance"
                    ))
                })?;
                if port > 0 {
                    let next_port =
                        port.checked_add(1).filter(|n| *n <= 65535).ok_or_else(|| {
                            Error::Config(format!(
                                "cannot increment {key}: port 65535 has no next port"
                            ))
                        })?;
                    next.set(key, &next_port.to_string())?;
                }
            }
        }
        let label = self.get("rust_instance_name").or_else(|| {
            path.and_then(|p| p.file_stem())
                .and_then(|s| s.to_str())
                .filter(|s| s.starts_with("VR") || s.starts_with("BR"))
        });
        if let Some(label) = label {
            let split = label.trim_end_matches(|c: char| c.is_ascii_digit()).len();
            let name = if split < label.len() {
                let index = label[split..]
                    .parse::<u32>()
                    .ok()
                    .and_then(|n| n.checked_add(1))
                    .ok_or_else(|| {
                        Error::Config("instance label number cannot be incremented".into())
                    })?;
                format!("{}{index}", &label[..split])
            } else {
                format!("{label} 2")
            };
            next.set("rust_instance_name", &name)?;
        }
        // File sources remain files. Copy gets no path, so Save cannot overwrite its parent.
        Ok((next, None))
    }
}
#[derive(Clone)]
pub struct DocumentHistory {
    pub text: String,
    undo: Vec<String>,
    redo: Vec<String>,
}
impl DocumentHistory {
    pub fn new(text: String) -> Self {
        Self {
            text,
            undo: vec![],
            redo: vec![],
        }
    }
    pub fn replace(&mut self, text: String) {
        if text == self.text {
            return;
        }
        if self.undo.len() >= 100 {
            self.undo.remove(0);
        }
        self.undo.push(std::mem::replace(&mut self.text, text));
        self.redo.clear();
    }
    pub fn undo(&mut self) -> bool {
        let Some(text) = self.undo.pop() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.text, text));
        true
    }
    pub fn redo(&mut self) -> bool {
        let Some(text) = self.redo.pop() else {
            return false;
        };
        self.undo.push(std::mem::replace(&mut self.text, text));
        true
    }
}
pub fn parameter_description(key: &str) -> (&'static str, &'static str) {
    match key {
        "rust_instance_name" => (
            "Instance",
            "Human-readable instance name. Copied VR instances increment their label, enabled UDP/ZMQ ports and numeric camera ID; video paths remain paths.",
        ),
        "rust_camera_width" | "rust_camera_height" => (
            "Camera",
            "Exact native capture dimensions. Select only advertised formats; a format change reopens the camera and requires matching calibration.",
        ),
        "src_fps" => (
            "Input",
            "Positive frame rate overrides the source clock; nonpositive uses native metadata or capture timestamps. GUI playback is paced to the selected source clock.",
        ),
        "src_fn" => (
            "Input",
            "Camera index or video path. File sources remain file sources in copied instances.",
        ),
        "vfov" => ("Geometry", "Camera vertical field of view in degrees."),
        "roi_c" | "roi_r" | "roi_circ" | "roi_ignr" => (
            "Geometry",
            "Saved sphere ray/radius take precedence over rim points; ignore polygons are in source pixels.",
        ),
        "c2a_r" => (
            "Geometry",
            "Rodrigues camera-to-animal rotation, radians. Axes: forward/right/down.",
        ),
        "q_factor" => (
            "Tracking",
            "Map quality 1..30: ROI is 10*q pixels square and equal-area atlas 30*q by 15*q. Changes fitting resolution as well as debug views.",
        ),
        "thr_ratio" | "thr_win_pc" => (
            "Tracking",
            "Binary sphere threshold: local masked minimum/maximum contrast ratio and window fraction, after 3x3 median filtering.",
        ),
        "thr_rgb_tfrm" => (
            "Tracking",
            "Select r, g, or b channel for thresholding; otherwise use integer BT.601 grayscale.",
        ),
        "opt_bound" => (
            "Tracking",
            "Per-axis local rotation search bound around the filtered prediction, radians.",
        ),
        "opt_tol" | "opt_max_evals" => (
            "Tracking",
            "BOBYQA final trust-region radius and bounded evaluation budget. Rust PRIMA differs from upstream NLopt.",
        ),
        "opt_max_err" => (
            "Tracking",
            "Reject fits above binary-map mean squared error; a nonpositive value disables this gate.",
        ),
        "opt_do_global" | "opt_global_evals" => (
            "Tracking",
            "Enable bounded global SO(3) atlas search and control its evaluation budget; used for initialization, resets and failed local recovery.",
        ),
        "max_bad_frames" => (
            "Tracking",
            "Maximum consecutive failed fits before reset/relocalization. Negative disables automatic reset.",
        ),
        "sphere_map_fn" => (
            "Tracking",
            "Initial equal-area sphere map image. Dimensions must match q_factor; 128 denotes unknown confidence. Relative paths resolve beside the config.",
        ),
        "rust_engine" => (
            "Tracking",
            "spherical (legacy default) or pairwise. Pairwise uses photometric luminance fitting with its separately documented error scale.",
        ),
        "fisheye" | "cam_model" => (
            "Geometry",
            "Source pinhole/equidistant camera projection. Calibration must match the actual source resolution and projection.",
        ),
        "c2a_cnrs" | "c2a_src" | "c2a_t" => (
            "Geometry",
            "Square XY/YZ/XZ calibration corners and pose. Saved c2a_r takes precedence; corner-only files are calibrated on import.",
        ),
        "do_display" => (
            "Debug",
            "Toggle native debug display; tracking and configured data/media outputs continue.",
        ),
        "save_raw" | "save_debug" => (
            "Recording",
            "Record source RGB / calibrated six-panel debug frames when a recording run is active. Encoded media finalize explicitly; incomplete files retain .partial.",
        ),
        "vid_codec" => (
            "Recording",
            "h264 writes pure Rust AVC in OpenDML AVI; even dimensions use edge padding and normative SPS cropping. xvid/mjpg/raw also write AVI; mpg4 and explicit h264_mp4 write MP4. Unsupported encoders fail explicitly when recording is requested.",
        ),
        "sock_host" | "sock_port" => (
            "Output",
            "UDP destination for accepted full 25-column rows prefixed FT, . Port -1/0 disables. Errors are surfaced without stopping numerical tracking.",
        ),
        "zmq_host" | "zmq_port" => (
            "Output",
            "Pure Rust ZeroMQ PUB bind on localhost. Port -1/0 disables; use a unique port per instance (VR1..4: 9871..9874). A single UTF-8 frame contains the identical accepted FT, 25-column row sent over UDP.",
        ),
        "zmq_queue_capacity" => (
            "Output",
            "Bounded asynchronous ZMQ queue capacity, 1..4096 (default 256). Overflow drops live messages and reports counters; tracking and numeric evidence continue.",
        ),
        "com_port" | "com_baud" => (
            "Output",
            "Native serial accepted-row output with FT, prefix, 8 data bits, no parity, one stop bit and no flow control. Baud defaults to 115200; empty port disables.",
        ),
        "output_fn" => (
            "Output",
            "Legacy output name retained for run naming; an explicit CLI or agent output directory takes precedence.",
        ),
        _ => (
            "Preserved",
            "Retained exactly for compatibility. Check runtime notices for unsupported execution semantics.",
        ),
    }
}
