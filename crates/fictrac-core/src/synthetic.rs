//! Analytic, deterministic textured sphere renderer with known SO(3) motion.
use crate::{Config, Geometry, GrayFrame, Result};
use glam::{DQuat, DVec3};

pub fn render(config: &Config, orientation: [f64; 3], timestamp_ms: f64) -> Result<GrayFrame> {
    let g = Geometry::new(config.clone())?;
    let q = DQuat::from_scaled_axis(DVec3::from_array(orientation)).inverse();
    let mut pixels = Vec::with_capacity(config.width * config.height);
    for y in 0..config.height {
        for x in 0..config.width {
            let value = if let Some(p) = g.surface(x as f64, y as f64) {
                let p = q * p;
                // Multiple incommensurate 3D waves; the pattern is fixed in material space.
                let v = (p.dot(DVec3::new(17.0, 11.0, 7.0))).sin()
                    + 0.7 * (p.dot(DVec3::new(-9.0, 23.0, 15.0))).cos()
                    + 0.5 * (p.dot(DVec3::new(31.0, -13.0, 19.0))).sin();
                0.5 + 0.43 * (2.0 * v).tanh()
            } else {
                0.12
            };
            pixels.push(value as f32);
        }
    }
    GrayFrame::new(config.width, config.height, pixels, timestamp_ms)
}
/// Constant-axis orientation at a frame index. Useful as an independent motion oracle.
pub fn orientation_at(delta: [f64; 3], index: usize) -> [f64; 3] {
    (DVec3::from_array(delta) * index as f64).to_array()
}
