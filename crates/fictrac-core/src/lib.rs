//! Headless, deterministic spherical motion tracking. No codecs or platform APIs.
pub mod calibration;
mod config;
mod diagnostics;
mod frame;
mod geometry;
mod spherical;
pub mod synthetic;
mod tracker;
pub use config::{Config, MapSettings, Sphere};
pub use frame::GrayFrame;
pub use geometry::Geometry;
pub use spherical::SphereTemplate;
pub use tracker::{TrackRow, Tracker};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("invalid frame: {0}")]
    Frame(String),
    #[error("tracking failed: {0}")]
    Tracking(String),
}
pub type Result<T> = std::result::Result<T, Error>;

pub use diagnostics::{DebugViews, Diagnostics};
