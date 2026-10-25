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
    if points.iter().any(|p| {
        p.iter().any(|v| !v.is_finite())
            || p[0] < 0.
            || p[1] < 0.
            || p[0] >= width as f64
            || p[1] >= height as f64
    }) {
        return Err(crate::Error::Config(
            "Square corners must be finite and inside the source image; check the source resolution and select TL, TR, BR, BL".into(),
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

/// Result of FicTrac's F reflection/re-fit operation. Corner order stays unchanged.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SquarePoseCycle {
    pub camera_to_lab: [f64; 3],
    pub translation: [f64; 3],
    /// Sum of squared differences between four observed and projected unit rays.
    pub ray_error: f64,
    pub orientation_change_rad: f64,
}
#[derive(Clone, Copy)]
struct SquarePose {
    rotation: DQuat,
    translation: DVec3,
}
/// Re-fit the square from upstream's reflected animal-to-camera third-column seed.
/// A second, geometric planar-ambiguity seed makes the bounded deterministic fit
/// useful when that improper-matrix seed returns to the existing solution.
/// Only a distinct proper rotation with a small ray reprojection error is accepted.
/// Some square views have no alternative minimum: this returns an explicit error.
pub fn cycle_square_pose(
    config: &Config,
    plane: &str,
    points: &[[f64; 2]],
    translation: [f64; 3],
) -> crate::Result<SquarePoseCycle> {
    config.validate()?;
    if points.len() != 4
        || points.iter().any(|p| {
            p.iter().any(|v| !v.is_finite())
                || p[0] < 0.
                || p[1] < 0.
                || p[0] >= config.width as f64
                || p[1] >= config.height as f64
        })
    {
        return Err(crate::Error::Config(
            "F square re-fit requires four finite corners inside the source image".into(),
        ));
    }
    let corners = square_reference(plane)?;
    let geometry = Geometry::new(config.clone())?;
    let rays = points
        .iter()
        .map(|p| geometry.ray(p[0], p[1]))
        .collect::<Vec<_>>();
    if rays.iter().any(|r| r.z <= 0.) {
        return Err(crate::Error::Config(
            "Square corners must be in the camera's forward hemisphere".into(),
        ));
    }
    let current = SquarePose {
        rotation: DQuat::from_scaled_axis(DVec3::from_array(config.camera_to_lab)).inverse(),
        translation: DVec3::from_array(translation),
    };
    if !current.translation.is_finite()
        || current.translation.z <= 0.
        || current.translation.abs().max_element() > 1e4
    {
        return Err(crate::Error::Config(
            "Saved c2a_t must be finite with positive camera depth and magnitude below 10000"
                .into(),
        ));
    }
    let base_error = pose_cost(current, &corners, &rays).ok_or_else(|| {
        crate::Error::Config("Saved square pose puts a corner behind the camera".into())
    })?;
    let mut reflected = DMat3::from_quat(current.rotation);
    reflected.z_axis = -reflected.z_axis;
    // Exactly upstream CmPoint::matrixToOmega on the reflected (improper) R seed.
    let angle = ((reflected.x_axis.x + reflected.y_axis.y + reflected.z_axis.z - 1.) * 0.5)
        .clamp(-1., 1.)
        .acos();
    let factor = if angle.sin() != 0. {
        angle / (2. * angle.sin())
    } else {
        angle
    };
    let seed = DVec3::new(
        reflected.y_axis.z - reflected.z_axis.y,
        reflected.z_axis.x - reflected.x_axis.z,
        reflected.x_axis.y - reflected.y_axis.x,
    ) * factor;
    let mut starts = vec![SquarePose {
        rotation: DQuat::from_scaled_axis(seed.clamp_length_max(std::f64::consts::PI)),
        translation: current.translation,
    }];
    let normal = current.rotation
        * match plane {
            "xy" => DVec3::Z,
            "yz" => DVec3::X,
            _ => DVec3::Y,
        };
    let view = current.translation.normalize();
    let mirror = (2. * view.dot(normal) * view - normal).normalize();
    if normal.dot(mirror) < 1. - 1e-12 {
        starts.push(SquarePose {
            rotation: DQuat::from_rotation_arc(normal, mirror) * current.rotation,
            translation: current.translation,
        });
    }
    let limit = (base_error * 4. + 1e-12).clamp(1e-4, 1e-3);
    let mut best: Option<(SquarePose, f64, f64)> = None;
    for start in starts {
        let pose = refit_square(start, &corners, &rays);
        let change = (pose.rotation * current.rotation.inverse())
            .to_scaled_axis()
            .length();
        if let Some(error) = pose_cost(pose, &corners, &rays)
            && change > 0.01
            && error <= limit
            && best.as_ref().is_none_or(|(_, old, _)| error < *old)
        {
            best = Some((pose, error, change));
        }
    }
    let (pose,ray_error,orientation_change_rad)=best.ok_or_else(||crate::Error::Config("F reflection re-fit found no distinct accurate square pose. Keep this orientation or re-select the corners/plane; this view may have only one usable solution.".into()))?;
    Ok(SquarePoseCycle {
        camera_to_lab: pose.rotation.inverse().to_scaled_axis().to_array(),
        translation: pose.translation.to_array(),
        ray_error,
        orientation_change_rad,
    })
}
fn square_reference(plane: &str) -> crate::Result<[DVec3; 4]> {
    let points = match plane {
        "xy" => [
            [0.5, -0.5, 0.],
            [0.5, 0.5, 0.],
            [-0.5, 0.5, 0.],
            [-0.5, -0.5, 0.],
        ],
        "yz" => [
            [0., -0.5, -0.5],
            [0., 0.5, -0.5],
            [0., 0.5, 0.5],
            [0., -0.5, 0.5],
        ],
        "xz" => [
            [0.5, 0., -0.5],
            [-0.5, 0., -0.5],
            [-0.5, 0., 0.5],
            [0.5, 0., 0.5],
        ],
        _ => {
            return Err(crate::Error::Config(
                "Square plane must be xy, yz or xz".into(),
            ));
        }
    };
    Ok(points.map(DVec3::from_array))
}
fn pose_cost(pose: SquarePose, corners: &[DVec3; 4], rays: &[DVec3]) -> Option<f64> {
    let mut error = 0.;
    for (point, ray) in corners.iter().zip(rays) {
        let p = pose.rotation * *point + pose.translation;
        if !p.is_finite() || p.z <= 1e-9 {
            return None;
        }
        error += (p.normalize() - *ray).length_squared();
    }
    error.is_finite().then_some(error)
}
fn refit_square(mut pose: SquarePose, corners: &[DVec3; 4], rays: &[DVec3]) -> SquarePose {
    let mut damping = 1e-6;
    for _ in 0..240 {
        let mut h = [[0.; 6]; 6];
        let mut b = [0.; 6];
        for (point, ray) in corners.iter().zip(rays) {
            let rotated = pose.rotation * *point;
            let p = rotated + pose.translation;
            let length = p.length();
            if !length.is_finite() || length < 1e-9 {
                return pose;
            }
            let v = p / length;
            let residual = v - *ray;
            let mut jacobian = [DVec3::ZERO; 6];
            for (index, j) in jacobian.iter_mut().enumerate() {
                let mut axis = DVec3::ZERO;
                axis[index % 3] = 1.;
                let dp = if index < 3 { axis.cross(rotated) } else { axis };
                *j = (dp - v * v.dot(dp)) / length;
            }
            for k in 0..6 {
                b[k] -= jacobian[k].dot(residual);
                for l in 0..6 {
                    h[k][l] += jacobian[k].dot(jacobian[l]);
                }
            }
        }
        for (index, row) in h.iter_mut().enumerate() {
            row[index] += damping;
        }
        let Some(step) = solve_pose(h, b) else {
            break;
        };
        let dr = DVec3::new(step[0], step[1], step[2]).clamp_length_max(0.5);
        let dt = DVec3::new(step[3], step[4], step[5])
            .clamp_length_max(pose.translation.z.max(1.) * 0.25);
        let candidate = SquarePose {
            rotation: (DQuat::from_scaled_axis(dr) * pose.rotation).normalize(),
            translation: pose.translation + dt,
        };
        let good = candidate.translation.abs().max_element() <= 1e4
            && pose_cost(candidate, corners, rays)
                .zip(pose_cost(pose, corners, rays))
                .is_some_and(|(next, old)| next < old);
        if good {
            pose = candidate;
            damping = (damping * 0.4).max(1e-16);
        } else {
            damping = (damping * 10.).min(1e12);
        }
        if dr.length_squared() + dt.length_squared() < 1e-24 {
            break;
        }
    }
    pose
}
fn solve_pose(h: [[f64; 6]; 6], b: [f64; 6]) -> Option<[f64; 6]> {
    let mut a = [[0.; 7]; 6];
    for i in 0..6 {
        a[i][..6].copy_from_slice(&h[i]);
        a[i][6] = b[i];
    }
    for i in 0..6 {
        let mut pivot = i;
        for k in i + 1..6 {
            if a[k][i].abs() > a[pivot][i].abs() {
                pivot = k;
            }
        }
        a.swap(i, pivot);
        let d = a[i][i];
        if !d.is_finite() || d.abs() < 1e-25 {
            return None;
        }
        for value in a[i].iter_mut().skip(i) {
            *value /= d;
        }
        let row = a[i];
        for (k, target) in a.iter_mut().enumerate() {
            if k == i {
                continue;
            }
            let factor = target[i];
            for j in i..7 {
                target[j] -= factor * row[j];
            }
        }
    }
    let x = a.map(|row| row[6]);
    x.iter().all(|v| v.is_finite()).then_some(x)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn square_rejects_outside_corners_and_crossed_order() {
        let square = [[80., 80.], [240., 80.], [240., 240.], [80., 240.]];
        for fisheye in [false, true] {
            let mut outside = square;
            outside[0][0] = -1.;
            assert!(lab_from_square(320, 320, 45., fisheye, "xy", &outside).is_err());
            outside[0][0] = 320.;
            assert!(lab_from_square(320, 320, 45., fisheye, "xy", &outside).is_err());
            let mut crossed = square;
            crossed.swap(2, 3);
            assert!(lab_from_square(320, 320, 45., fisheye, "xy", &crossed).is_err());
        }
    }
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
    #[test]
    fn f_square_refit_finds_real_alternate_and_cycles_back_without_reordering() -> crate::Result<()>
    {
        for fisheye in [false, true] {
            for plane in ["xy", "yz", "xz"] {
                let base = match plane {
                    "xy" => DQuat::IDENTITY,
                    "yz" => DQuat::from_rotation_y(-std::f64::consts::FRAC_PI_2),
                    _ => DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2),
                };
                let rotation = DQuat::from_scaled_axis(DVec3::new(0.4, 0.2, 0.1)) * base;
                let t = DVec3::new(0.2, -0.1, 20.);
                let mut config = Config {
                    width: 320,
                    height: 320,
                    vfov_deg: 45.,
                    fisheye,
                    camera_to_lab: rotation.inverse().to_scaled_axis().to_array(),
                    ..Config::default()
                };
                let geometry = Geometry::new(config.clone())?;
                let points = square_reference(plane)?
                    .iter()
                    .map(|point| {
                        geometry.project_ray(rotation * *point + t).ok_or_else(|| {
                            crate::Error::Config("projected test corner is outside image".into())
                        })
                    })
                    .collect::<crate::Result<Vec<_>>>()?;
                let original = points.clone();
                let alternate = cycle_square_pose(&config, plane, &points, t.to_array())?;
                assert!(alternate.ray_error < 3e-6, "{}", alternate.ray_error);
                assert!(alternate.orientation_change_rad > 0.4);
                let alternate_rotation = DMat3::from_quat(DQuat::from_scaled_axis(
                    DVec3::from_array(alternate.camera_to_lab),
                ));
                assert!((alternate_rotation.determinant() - 1.).abs() < 1e-12);
                assert_eq!(points, original);
                config.camera_to_lab = alternate.camera_to_lab;
                let restored = cycle_square_pose(&config, plane, &points, alternate.translation)?;
                let difference =
                    (DQuat::from_scaled_axis(DVec3::from_array(restored.camera_to_lab)) * rotation)
                        .to_scaled_axis()
                        .length();
                assert!(difference < 1e-5, "{plane} fisheye {fisheye}: {difference}");
                assert!(restored.ray_error < 1e-14);
            }
        }
        Ok(())
    }
    #[test]
    fn f_square_refit_reports_no_branch_for_frontal_square_and_rejects_bad_pose()
    -> crate::Result<()> {
        let config = Config {
            width: 320,
            height: 320,
            vfov_deg: 45.,
            ..Config::default()
        };
        let geometry = Geometry::new(config.clone())?;
        let t = DVec3::new(0., 0., 5.);
        let points = square_reference("xy")?
            .iter()
            .map(|point| {
                geometry
                    .project_ray(*point + t)
                    .ok_or_else(|| crate::Error::Config("test projection failed".into()))
            })
            .collect::<crate::Result<Vec<_>>>()?;
        assert!(cycle_square_pose(&config, "xy", &points, t.to_array()).is_err());
        assert!(cycle_square_pose(&config, "xy", &points, [0., 0., -1.]).is_err());
        assert!(cycle_square_pose(&config, "xy", &points[..3], t.to_array()).is_err());
        Ok(())
    }
}
