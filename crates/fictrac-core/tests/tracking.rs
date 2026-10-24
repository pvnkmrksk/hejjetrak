use fictrac_core::{Config, Geometry, GrayFrame, Tracker, synthetic};
use glam::{DQuat, DVec3};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

#[test]
fn recovers_each_axis_and_combined_motion() -> Result {
    let config = Config::default();
    for delta in [
        [0.012, 0.0, 0.0],
        [0.0, -0.014, 0.0],
        [0.0, 0.0, 0.018],
        [0.008, -0.011, 0.005],
    ] {
        let mut tracker = Tracker::new(config.clone())?;
        for i in 0..12 {
            let row = tracker.process(synthetic::render(
                &config,
                synthetic::orientation_at(delta, i),
                i as f64 * 10.0,
            )?)?;
            assert!(
                row.accepted,
                "{delta:?} at {i}: {} error {}",
                row.status, row.error
            );
            if i > 0 {
                let error =
                    (DVec3::from_array(row.delta_camera) - DVec3::from_array(delta)).length();
                assert!(
                    error < 0.002,
                    "expected {delta:?}, got {:?}, error {error}",
                    row.delta_camera
                );
            }
        }
    }
    Ok(())
}
#[test]
fn recovers_larger_rotation_with_coarse_levels() -> Result {
    let config = Config::default();
    let mut tracker = Tracker::new(config.clone())?;
    tracker.process(synthetic::render(&config, [0.0; 3], 0.0)?)?;
    let expected = [0.065, -0.08, 0.025];
    let row = tracker.process(synthetic::render(&config, expected, 10.0)?)?;
    assert!(row.accepted);
    assert!(
        (DVec3::from_array(row.delta_camera) - DVec3::from_array(expected)).length() < 0.005,
        "{:?}",
        row.delta_camera
    );
    Ok(())
}
#[test]
fn stationary_sphere_has_zero_motion() -> Result {
    let config = Config::default();
    let mut tracker = Tracker::new(config.clone())?;
    for i in 0..3 {
        let row = tracker.process(synthetic::render(&config, [0.0; 3], i as f64)?)?;
        assert!(row.accepted);
        assert!(row.delta_camera.iter().all(|v| v.abs() < 1e-9));
        assert_eq!(row.position, [0.0; 2]);
    }
    Ok(())
}
#[test]
fn flat_frames_are_rejected_without_changing_path() -> Result {
    let config = Config::default();
    let mut tracker = Tracker::new(config.clone())?;
    tracker.process(synthetic::render(&config, [0.0; 3], 0.0)?)?;
    for i in 1..4 {
        let row = tracker.process(GrayFrame::new(
            config.width,
            config.height,
            vec![0.5; config.width * config.height],
            i as f64,
        )?)?;
        assert!(!row.accepted);
        assert_eq!(row.position, [0.0; 2]);
        assert_eq!(row.delta_camera, [0.0; 3]);
        assert!(row.error.is_finite());
    }
    Ok(())
}
#[test]
fn timestamp_failures_do_not_mutate_tracker() -> Result {
    let config = Config::default();
    let mut tracker = Tracker::new(config.clone())?;
    tracker.process(synthetic::render(&config, [0.0; 3], 10.0)?)?;
    assert!(
        tracker
            .process(synthetic::render(&config, [0.0; 3], 10.0)?)
            .is_err()
    );
    assert!(
        tracker
            .process(synthetic::render(&config, [0.0; 3], 9.0)?)
            .is_err()
    );
    let row = tracker.process(synthetic::render(&config, [0.0; 3], 20.0)?)?;
    assert_eq!(row.frame, 2);
    assert_eq!(row.delta_ms, 10.0);
    Ok(())
}
#[test]
fn rotates_into_lab_coordinates_and_integrates_forward() -> Result {
    let config = Config {
        camera_to_lab: [0.0, 0.0, std::f64::consts::FRAC_PI_2],
        ..Config::default()
    };
    let delta = [0.01, 0.0, 0.0];
    let mut tracker = Tracker::new(config.clone())?;
    for i in 0..11 {
        let row = tracker.process(synthetic::render(
            &config,
            synthetic::orientation_at(delta, i),
            i as f64 * 10.0,
        )?)?;
        if i == 10 {
            assert!((row.position[0] - 0.10).abs() < 0.005, "{:?}", row.position);
            assert!(row.position[1].abs() < 0.005);
            let expected = DQuat::from_scaled_axis(DVec3::from_array(config.camera_to_lab))
                * DVec3::from_array(row.delta_camera);
            assert!((DVec3::from_array(row.delta_lab) - expected).length() < 1e-10);
        }
    }
    Ok(())
}
#[test]
fn fits_calibrated_perspective_rim() -> Result {
    let config = Config::default();
    let axis = DVec3::new(-0.16, 0.12, 1.0).normalize();
    let u = axis.cross(DVec3::Y).normalize();
    let v = axis.cross(u);
    let radius: f64 = 0.19;
    let focal = config.height as f64 * 0.5 / (config.vfov_deg.to_radians() * 0.5).tan();
    let points: Vec<_> = (0..12)
        .map(|i| {
            let angle = i as f64 * std::f64::consts::TAU / 12.0;
            let r = axis * radius.cos() + radius.sin() * (u * angle.cos() + v * angle.sin());
            [focal * r.x / r.z + 80.0, focal * r.y / r.z + 80.0]
        })
        .collect();
    let sphere = Geometry::fit_sphere(160, 160, 45.0, &points)?;
    assert!((DVec3::from_array(sphere.centre) - axis).length() < 1e-8);
    assert!((sphere.angular_radius - radius).abs() < 1e-8);
    Ok(())
}
#[test]
fn rejects_degenerate_and_hostile_calibration() {
    for points in [
        vec![],
        vec![[1.0, 1.0]; 3],
        vec![[20.0, 30.0], [30.0, 30.0], [40.0, 30.0]],
        vec![[f64::NAN, 1.0]; 3],
    ] {
        assert!(Geometry::fit_sphere(160, 160, 45.0, &points).is_err());
    }
    for n in [0, 1, usize::MAX] {
        assert!(
            Tracker::new(Config {
                width: n,
                ..Config::default()
            })
            .is_err()
        );
    }
    for value in [f64::NAN, f64::INFINITY, 0.0, -1.0] {
        assert!(
            Tracker::new(Config {
                max_rotation: value,
                ..Config::default()
            })
            .is_err()
        );
    }
}
#[test]
fn rejects_bad_frames_and_samples_safely() -> Result {
    assert!(GrayFrame::new(2, 2, vec![f32::NAN; 4], 0.0).is_err());
    assert!(GrayFrame::new(usize::MAX, 2, vec![], 0.0).is_err());
    assert!(GrayFrame::new(2, 2, vec![0.0; 3], 0.0).is_err());
    let frame = GrayFrame::new(2, 2, vec![0.0, 1.0, 1.0, 0.0], 0.0)?;
    assert!((frame.sample(0.5, 0.5).ok_or("sample missing")? - 0.5).abs() < 1e-9);
    assert!(frame.sample(f64::NAN, 1.0).is_none());
    assert!(frame.sample(-1.0, 0.0).is_none());
    assert!(frame.sample(2.0, 2.0).is_none());
    Ok(())
}
#[test]
fn off_axis_sphere_is_tracked() -> Result {
    let mut config = Config::default();
    config.sphere.centre = DVec3::new(-0.14, 0.08, 1.0).normalize().to_array();
    config.sphere.angular_radius = 0.20;
    let mut tracker = Tracker::new(config.clone())?;
    let delta = [-0.007, 0.012, -0.004];
    tracker.process(synthetic::render(&config, [0.0; 3], 0.0)?)?;
    let row = tracker.process(synthetic::render(&config, delta, 10.0)?)?;
    assert!(row.accepted);
    assert!((DVec3::from_array(row.delta_camera) - DVec3::from_array(delta)).length() < 0.002);
    Ok(())
}
#[test]
fn tracking_is_deterministic() -> Result {
    let c = Config::default();
    let mut a = Tracker::new(c.clone())?;
    let mut b = Tracker::new(c.clone())?;
    for i in 0..4 {
        let f = synthetic::render(&c, [0.003 * i as f64, 0.0, 0.001 * i as f64], i as f64)?;
        let x = a.process(f.clone())?;
        let y = b.process(f)?;
        assert_eq!(x.delta_camera, y.delta_camera);
        assert_eq!(x.position, y.position);
    }
    Ok(())
}

#[test]
fn exclusion_masks_prevent_an_occluder_from_driving_motion() -> Result {
    let config = Config {
        ignore_polygons: vec![vec![
            [40.0, 30.0],
            [115.0, 30.0],
            [115.0, 65.0],
            [40.0, 65.0],
        ]],
        ..Config::default()
    };
    let mut tracker = Tracker::new(config.clone())?;
    let delta = [0.008, -0.012, 0.004];
    for i in 0..6 {
        let frame = synthetic::render(
            &config,
            synthetic::orientation_at(delta, i),
            i as f64 * 10.0,
        )?;
        let mut pixels = frame.pixels().to_vec();
        for y in 30..65 {
            for x in 40..115 {
                pixels[y * config.width + x] = if i % 2 == 0 { 0.0 } else { 1.0 };
            }
        }
        let row = tracker.process(GrayFrame::new(
            config.width,
            config.height,
            pixels,
            frame.timestamp_ms(),
        )?)?;
        assert!(row.accepted, "{i}: {}", row.error);
        if i > 0 {
            assert!(
                (DVec3::from_array(row.delta_camera) - DVec3::from_array(delta)).length() < 0.003
            );
        }
    }
    Ok(())
}
#[test]
fn composes_noncommuting_rotations_on_so3() -> Result {
    let config = Config::default();
    let mut tracker = Tracker::new(config.clone())?;
    let mut orientation = DQuat::IDENTITY;
    for i in 0..15 {
        if i > 0 {
            let delta = if i % 2 == 0 {
                DVec3::new(0.012, 0.0, 0.0)
            } else {
                DVec3::new(0.0, -0.015, 0.006)
            };
            orientation = (DQuat::from_scaled_axis(delta) * orientation).normalize();
        }
        let row = tracker.process(synthetic::render(
            &config,
            orientation.to_scaled_axis().to_array(),
            i as f64 * 10.0,
        )?)?;
        assert!(row.accepted);
        let actual = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
        assert!((actual * orientation.inverse()).to_scaled_axis().length() < 0.006);
    }
    Ok(())
}
#[test]
fn rotation_at_search_bound_is_rejected() -> Result {
    let config = Config {
        max_rotation: 0.003,
        ..Config::default()
    };
    let mut tracker = Tracker::new(config.clone())?;
    tracker.process(synthetic::render(&config, [0.0; 3], 0.0)?)?;
    let row = tracker.process(synthetic::render(&config, [0.03, 0.01, 0.0], 10.0)?)?;
    assert!(!row.accepted);
    assert_eq!(row.delta_camera, [0.0; 3]);
    assert_eq!(row.position, [0.0; 2]);
    Ok(())
}

#[test]
fn legacy_exports_use_the_original_recentered_roi_basis() -> Result {
    let mut config = Config {
        legacy_roi_coordinates: true,
        ..Config::default()
    };
    config.sphere.centre = DVec3::new(-0.14, 0.08, 1.0).normalize().to_array();
    config.sphere.angular_radius = 0.20;
    let basis = DQuat::from_rotation_arc(DVec3::from_array(config.sphere.centre), DVec3::Z);
    let mut tracker = Tracker::new(config.clone())?;
    let first = tracker.process(synthetic::render(&config, [0.0; 3], 0.0)?)?;
    assert_eq!(first.frame, 0);
    let delta = [0.008, -0.012, 0.004];
    let row = tracker.process(synthetic::render(&config, delta, 10.0)?)?;
    assert!(
        (DVec3::from_array(row.delta_camera) - basis * DVec3::from_array(delta)).length() < 0.002
    );
    Ok(())
}
#[test]
fn equidistant_fisheye_known_motion_is_recovered() -> Result {
    let config = Config {
        fisheye: true,
        ..Config::default()
    };
    let mut tracker = Tracker::new(config.clone())?;
    tracker.process(synthetic::render(&config, [0.0; 3], 0.0)?)?;
    let delta = [0.008, -0.012, 0.004];
    let row = tracker.process(synthetic::render(&config, delta, 10.0)?)?;
    assert!(row.accepted);
    assert!((DVec3::from_array(row.delta_camera) - DVec3::from_array(delta)).length() < 0.002);
    Ok(())
}
