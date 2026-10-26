//! File adapters; the tracking engine depends on none of these codecs.
mod calibration_reference;
mod comparison;
mod config;
mod document;
mod output;
mod prefetch;
mod video;
pub use calibration_reference::{capture_calibration_reference, rescale_calibration_reference};
pub use config::{LoadedConfig, load_config, load_config_text, load_source_hint};
pub use output::{write_dat, write_path_svg};
pub use prefetch::PrefetchedVideoSource;
pub use video::{VideoInfo, VideoSource, rgb_tracking_frame, write_y4m_frame, write_y4m_header};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Core(#[from] fictrac_core::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("video error: {0}")]
    Video(String),
    #[error("configuration error: {0}")]
    Config(String),
}
pub type Result<T> = std::result::Result<T, Error>;

pub use document::{ConfigDocument, DocumentHistory, SourceSpec, parameter_description};

pub use comparison::{Comparison, compare_dat};

mod debug_render;
mod recording;
mod sinks;
mod templates;
pub use debug_render::{CalibratedDebugRenderer, render_calibrated_debug_rgb, render_debug_rgb};
pub use recording::{RecordingCodec, VideoRecorder};
pub use sinks::{OutputSinks, OutputStatus};
pub use templates::{load_sphere_map, save_sphere_map};
pub use zmq_output::ZmqOutputStatus;
pub use zmq_payload::{DEFAULT_ZMQ_TOPIC, ZmqFraming, ZmqOutputMode, ZmqPayloadSettings};

mod mp4_recording;

mod avi;

mod codecs;

mod avi_segments;

mod avi_mux;

pub mod h264_crop;

mod zmq_output;
mod zmq_payload;
