//! Desktop adapters for the shared headless calibration engine.
use fictrac_core::{Config, Sphere};
pub fn lab_from_square(
    width: usize,
    height: usize,
    vfov: f64,
    fisheye: bool,
    plane: &str,
    points: &[[f64; 2]],
) -> Result<([f64; 3], [f64; 3]), String> {
    fictrac_core::calibration::lab_from_square(width, height, vfov, fisheye, plane, points)
        .map_err(|e| e.to_string())
}
pub fn fit_rim(config: &Config, points: &[[f64; 2]]) -> Result<Sphere, String> {
    fictrac_core::calibration::fit_rim(config, points).map_err(|e| e.to_string())
}
