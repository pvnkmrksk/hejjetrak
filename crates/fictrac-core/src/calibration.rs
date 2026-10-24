use crate::{Config, Geometry};
use glam::{DMat3, DQuat, DVec3};
type Result<T> = std::result::Result<T, String>;
fn square(
    width: usize,
    height: usize,
    vfov: f64,
    fisheye: bool,
    plane: &str,
    points: &[[f64; 2]],
) -> Result<([f64; 3], [f64; 3])> {
    if points.len() != 4 {
        return Err("Select four square corners in order: TL, TR, BR, BL".into());
    }
    let f = height as f64 * 0.5 / (vfov.to_radians() * 0.5).tan();
    if !f.is_finite() || f <= 0.0 {
        return Err("invalid vfov".into());
    }
    let uv = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    let mut matrix = [[0.0; 9]; 8];
    for i in 0..4 {
        let [x, y] = points[i];
        if !x.is_finite() || !y.is_finite() {
            return Err("nonfinite corner".into());
        }
        let dx = x - width as f64 * 0.5;
        let dy = y - height as f64 * 0.5;
        let factor = if fisheye {
            let radius = dx.hypot(dy);
            let angle = radius * vfov.to_radians() / height as f64;
            if angle >= std::f64::consts::FRAC_PI_2 - 1e-6 {
                return Err("Square corners must be in the camera's forward hemisphere".into());
            }
            if radius < 1e-10 {
                vfov.to_radians() / height as f64
            } else {
                angle.tan() / radius
            }
        } else {
            1.0 / f
        };
        let x = dx * factor;
        let y = dy * factor;
        let [u, v] = uv[i];
        matrix[2 * i] = [u, v, 1.0, 0.0, 0.0, 0.0, -x * u, -x * v, x];
        matrix[2 * i + 1] = [0.0, 0.0, 0.0, u, v, 1.0, -y * u, -y * v, y];
    }
    for i in 0..8 {
        let pivot = (i..8)
            .max_by(|a, b| matrix[*a][i].abs().total_cmp(&matrix[*b][i].abs()))
            .ok_or("degenerate square")?;
        matrix.swap(i, pivot);
        let d = matrix[i][i];
        if d.abs() < 1e-10 {
            return Err("Degenerate square; corners must span a visible plane".into());
        }
        for value in matrix[i].iter_mut().skip(i) {
            *value /= d;
        }
        let pivot_row = matrix[i];
        for (k, row) in matrix.iter_mut().enumerate() {
            if k == i {
                continue;
            }
            let d = row[i];
            for (value, pivot) in row.iter_mut().zip(pivot_row).skip(i) {
                *value -= d * pivot;
            }
        }
    }
    let h = matrix.map(|row| row[8]);
    let a = DVec3::new(h[0], h[3], h[6]);
    let b = DVec3::new(h[1], h[4], h[7]);
    let scale = 1.0 / (a.length() + b.length());
    if !scale.is_finite() {
        return Err("invalid square scale".into());
    }
    let u = a.normalize();
    let v = (b - u * b.dot(u)).normalize();
    let (x, y, z) = match plane {
        "xy" => {
            let x = -v;
            let y = u;
            (x, y, x.cross(y))
        }
        "yz" => {
            let y = u;
            let z = v;
            (y.cross(z), y, z)
        }
        "xz" => {
            let x = -u;
            let z = v;
            (x, z.cross(x), z)
        }
        _ => return Err("unknown calibration plane".into()),
    };
    let rotation = DMat3::from_cols(x, y, z).transpose();
    let r = DQuat::from_mat3(&rotation).to_scaled_axis();
    if !r.is_finite() {
        return Err("invalid lab orientation".into());
    }
    // Upstream's reference square has unit edges (corners ±0.5).
    Ok((
        r.to_array(),
        (DVec3::new(h[2], h[5], 1.0) * scale).to_array(),
    ))
}
fn forward(config: &Config, start: [f64; 2], end: [f64; 2]) -> Result<[f64; 3]> {
    let old = DQuat::from_scaled_axis(DVec3::from_array(config.camera_to_lab));
    let basis = if config.legacy_roi_coordinates {
        DQuat::from_rotation_arc(DVec3::from_array(config.sphere.centre), DVec3::Z)
    } else {
        DQuat::IDENTITY
    };
    let down = old.inverse() * DVec3::Z;
    let desired = basis * DVec3::new(end[0] - start[0], end[1] - start[1], 0.0);
    let forward = (desired - down * desired.dot(down))
        .try_normalize()
        .ok_or("forward arrow is degenerate; use square calibration for this viewing angle")?;
    let right = down.cross(forward).normalize();
    let rotation = DMat3::from_cols(forward, right, down).transpose();
    Ok(DQuat::from_mat3(&rotation).to_scaled_axis().to_array())
}
/// Fit FicTrac's unit square with its legacy TL/TR/BR/BL corner ordering.
pub fn lab_from_square(
    width: usize,
    height: usize,
    vfov: f64,
    fisheye: bool,
    plane: &str,
    points: &[[f64; 2]],
) -> crate::Result<([f64; 3], [f64; 3])> {
    if !(16..=8192).contains(&width)
        || !(16..=8192).contains(&height)
        || !vfov.is_finite()
        || !(1.0..170.0).contains(&vfov)
    {
        return Err(crate::Error::Config(
            "invalid square camera dimensions or field of view".into(),
        ));
    }
    square(width, height, vfov, fisheye, plane, points).map_err(crate::Error::Config)
}
/// Change the forward direction while preserving the calibrated down vector.
pub fn forward_rotation(
    config: &Config,
    start: [f64; 2],
    end: [f64; 2],
) -> crate::Result<[f64; 3]> {
    config.validate()?;
    if start.iter().chain(end.iter()).any(|v| !v.is_finite()) {
        return Err(crate::Error::Config("nonfinite forward arrow".into()));
    }
    forward(config, start, end).map_err(crate::Error::Config)
}
pub fn fit_rim(config: &Config, points: &[[f64; 2]]) -> crate::Result<crate::Sphere> {
    Geometry::fit_sphere_model(
        config.width,
        config.height,
        config.vfov_deg,
        points,
        config.fisheye,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_square_planes_and_camera_models_recover_pose_and_unit_scale() -> crate::Result<()> {
        let rotation = DQuat::from_scaled_axis(DVec3::new(0.31, -0.42, 0.18));
        let translation = DVec3::new(0.2, -0.1, 6.0);
        for fisheye in [false, true] {
            for plane in ["xy", "yz", "xz"] {
                let corners = match plane {
                    "xy" => [
                        [0.5, -0.5, 0.0],
                        [0.5, 0.5, 0.0],
                        [-0.5, 0.5, 0.0],
                        [-0.5, -0.5, 0.0],
                    ],
                    "yz" => [
                        [0.0, -0.5, -0.5],
                        [0.0, 0.5, -0.5],
                        [0.0, 0.5, 0.5],
                        [0.0, -0.5, 0.5],
                    ],
                    _ => [
                        [0.5, 0.0, -0.5],
                        [-0.5, 0.0, -0.5],
                        [-0.5, 0.0, 0.5],
                        [0.5, 0.0, 0.5],
                    ],
                };
                let points: Vec<_> = corners
                    .iter()
                    .map(|p| {
                        let p = rotation * DVec3::from_array(*p) + translation;
                        let factor = if fisheye {
                            let radius = p.x.hypot(p.y);
                            320.0 / 45f64.to_radians() * radius.atan2(p.z) / radius
                        } else {
                            160.0 / 22.5f64.to_radians().tan() / p.z
                        };
                        [160.0 + factor * p.x, 160.0 + factor * p.y]
                    })
                    .collect();
                let (actual, t) = lab_from_square(320, 320, 45.0, fisheye, plane, &points)?;
                assert!(
                    (DQuat::from_scaled_axis(DVec3::from_array(actual)) * rotation)
                        .to_scaled_axis()
                        .length()
                        < 1e-7,
                    "{plane} fisheye {fisheye}"
                );
                assert!(
                    (DVec3::from_array(t) - translation).length() < 1e-7,
                    "unit square scale {plane}"
                );
            }
        }
        Ok(())
    }
    #[test]
    fn square_recovers_known_xy_pose() -> crate::Result<()> {
        let r = DQuat::from_scaled_axis(DVec3::new(0.2, -0.3, 0.1));
        let t = DVec3::new(0.2, -0.1, 7.0);
        let f = 160.0 / (22.5f64.to_radians().tan());
        let points: Vec<_> = [
            DVec3::new(1.0, -1.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            DVec3::new(-1.0, 1.0, 0.0),
            DVec3::new(-1.0, -1.0, 0.0),
        ]
        .iter()
        .map(|p| {
            let p = r * (*p * 0.5) + t;
            [f * p.x / p.z + 160.0, f * p.y / p.z + 160.0]
        })
        .collect();
        let (actual, _) = lab_from_square(320, 320, 45.0, false, "xy", &points)?;
        assert!(
            (DQuat::from_scaled_axis(DVec3::from_array(actual)) * r)
                .to_scaled_axis()
                .length()
                < 1e-7
        );
        Ok(())
    }
}
