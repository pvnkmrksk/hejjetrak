use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sphere {
    /// Unit camera ray toward sphere centre (camera x right, y down, z forward).
    pub centre: [f64; 3],
    /// Apparent angular radius, radians; physical radius / distance = sin(radius).
    pub angular_radius: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub spherical_map: Option<MapSettings>,
    pub width: usize,
    pub height: usize,
    pub vfov_deg: f64,
    pub fisheye: bool,
    pub sphere: Sphere,
    /// Rodrigues axis-angle taking camera vectors to animal/lab vectors.
    pub camera_to_lab: [f64; 3],
    /// Match FicTrac v2 exports: its camera-labelled vectors use the recentered ROI basis.
    pub legacy_roi_coordinates: bool,
    pub ignore_polygons: Vec<Vec<[f64; 2]>>,
    /// Pairwise: per-frame rotation-norm search limit. Spherical map: each
    /// ROI-axis search half-window around the prediction (legacy opt_bound),
    /// not a universal maximum physical angular velocity. Resilient tracking
    /// adds a recent-motion gate with an explicit per-frame allowance.
    pub max_rotation: f64,
    /// Normalised luminance RMSE (0..1), not the original FicTrac error metric.
    pub max_error: f64,
    pub sample_stride: usize,
    pub iterations: usize,
    pub source: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            spherical_map: None,
            width: 160,
            height: 160,
            vfov_deg: 45.0,
            fisheye: false,
            sphere: Sphere {
                centre: [0.0, 0.0, 1.0],
                angular_radius: 0.29,
            },
            camera_to_lab: [0.0; 3],
            legacy_roi_coordinates: false,
            ignore_polygons: vec![],
            max_rotation: 0.35,
            max_error: 0.16,
            sample_stride: 2,
            iterations: 20,
            source: None,
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if let Some(m) = &self.spherical_map
            && (!m.resilient_error_floor.is_finite()
                || !(10_000.0..=18_000.0).contains(&m.resilient_error_floor))
        {
            return Err(Error::Config(
                "spherical-map resilient_error_floor must be finite and in 10000..=18000 binary-map MSE".into(),
            ));
        }
        if let Some(m) = &self.spherical_map
            && (m.channel.is_some_and(|c| c > 2)
                || !(1..=30).contains(&m.quality)
                || !(8..=10000).contains(&m.max_evaluations)
                || !m.threshold_ratio.is_finite()
                || !(0.01..=100.0).contains(&m.threshold_ratio)
                || !m.threshold_window.is_finite()
                || !(0.0..=1.0).contains(&m.threshold_window)
                || !m.tolerance.is_finite()
                || m.tolerance <= 0.0
                || m.tolerance >= self.max_rotation * 0.5
                || !m.max_error.is_finite()
                || !(-1..=1_000_000).contains(&m.max_bad_frames)
                || !(256..=1_000_000).contains(&m.global_evaluations))
        {
            return Err(Error::Config(
                "invalid spherical-map optimizer/threshold settings".into(),
            ));
        }

        if self.width < 16
            || self.height < 16
            || self.width > 8192
            || self.height > 8192
            || self
                .width
                .checked_mul(self.height)
                .is_none_or(|n| n > 32_000_000)
        {
            return Err(Error::Config(
                "dimensions must be 16..8192, at most 32 million pixels".into(),
            ));
        }
        let c = glam::DVec3::from_array(self.sphere.centre);
        if !c.is_finite() || (c.length() - 1.0).abs() > 1e-5 || c.z <= 0.0 {
            return Err(Error::Config(
                "sphere centre must be a unit ray in front of the camera".into(),
            ));
        }
        if !self.vfov_deg.is_finite()
            || !(1.0..170.0).contains(&self.vfov_deg)
            || !self.sphere.angular_radius.is_finite()
            || !(0.001..1.4).contains(&self.sphere.angular_radius)
            || !glam::DVec3::from_array(self.camera_to_lab).is_finite()
            || self.camera_to_lab.iter().any(|x| x.abs() > 100.0)
        {
            return Err(Error::Config(
                "invalid field of view, sphere radius or lab rotation".into(),
            ));
        }
        if !self.max_rotation.is_finite()
            || !(0.001..1.0).contains(&self.max_rotation)
            || !self.max_error.is_finite()
            || !(0.001..=1.0).contains(&self.max_error)
            || !(1..=16).contains(&self.sample_stride)
            || !(1..=100).contains(&self.iterations)
        {
            return Err(Error::Config("invalid optimiser limits".into()));
        }
        if self.ignore_polygons.len() > 64
            || self.ignore_polygons.iter().any(|p| {
                !(3..=256).contains(&p.len())
                    || p.iter()
                        .flatten()
                        .any(|x| !x.is_finite() || x.abs() > 100_000.0)
            })
        {
            return Err(Error::Config("invalid exclusion polygons".into()));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MapSettings {
    /// Protect learned evidence and confirm bounded reacquisition after occlusion.
    /// Disable explicitly for the original permissive/reset policy.
    pub resilient: bool,
    /// Lower bound for the resilient adaptive complete-ROI MSE ceiling.
    /// Scene-specific opt-in, 10000..=18000; max_error remains a hard cap.
    pub resilient_error_floor: f64,
    /// Search the learned atlas after failed local fitting, and on reset/template load.
    pub global_search: bool,
    /// Reset after more than this many consecutive failures; -1 disables automatic reset.
    pub max_bad_frames: i32,
    /// Total bounded objective-evaluation budget for deterministic SO(3) relocalisation.
    pub global_evaluations: usize,
    pub channel: Option<usize>,
    pub quality: usize,
    pub threshold_ratio: f64,
    pub threshold_window: f64,
    pub max_evaluations: usize,
    pub tolerance: f64,
    pub max_error: f64,
}
impl Default for MapSettings {
    fn default() -> Self {
        Self {
            resilient: true,
            resilient_error_floor: 10_000.0,
            global_search: false,
            max_bad_frames: -1,
            global_evaluations: 100_000,
            channel: None,
            quality: 6,
            threshold_ratio: 1.25,
            threshold_window: 0.25,
            max_evaluations: 100,
            tolerance: 0.001,
            max_error: -1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resilient_error_floor_is_finite_bounded_and_defaults_to_existing_policy() {
        let mut config = Config {
            spherical_map: Some(MapSettings::default()),
            ..Config::default()
        };
        assert_eq!(MapSettings::default().resilient_error_floor, 10_000.0);
        for floor in [10_000.0, 15_000.0, 18_000.0] {
            if let Some(map) = &mut config.spherical_map {
                map.resilient_error_floor = floor;
            }
            assert!(config.validate().is_ok(), "valid floor {floor}");
        }
        for floor in [
            9_999.0,
            18_001.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            if let Some(map) = &mut config.spherical_map {
                map.resilient_error_floor = floor;
            }
            assert!(config.validate().is_err(), "invalid floor {floor}");
        }
    }
}
