use fictrac_core::{Config, Geometry, GrayFrame, MapSettings, SphereTemplate, Tracker, synthetic};
use glam::{DQuat, DVec3};
use std::f64::consts::{PI, TAU};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;

fn config(global: bool) -> Config {
    Config {
        spherical_map: Some(MapSettings {
            // These pre-existing cases explicitly exercise legacy global/reset
            // policy. Separate sustained-occlusion cases cover resilient mode.
            resilient: false,
            global_search: global,
            global_evaluations: 100_000,
            max_error: 12_000.0,
            max_evaluations: 150,
            ..MapSettings::default()
        }),
        legacy_roi_coordinates: true,
        ..Config::default()
    }
}
fn index(p: DVec3, width: usize, height: usize) -> usize {
    let lon = p.x.atan2(p.z);
    let x = (((-lon - PI) / TAU).rem_euclid(1.0) * width as f64).floor() as usize;
    let y = ((p.y + 1.0) * 0.5 * height as f64).floor() as usize;
    y.min(height - 1) * width + x.min(width - 1)
}
fn known_template() -> fictrac_core::Result<SphereTemplate> {
    let (w, h) = (180, 90);
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let py = (y as f64 + 0.5) / h as f64 * 2.0 - 1.0;
            let lon = -(x as f64 + 0.5) / w as f64 * TAU - PI;
            let r = (1.0 - py * py).sqrt();
            let p = DVec3::new(r * lon.sin(), py, r * lon.cos());
            let v = (p.dot(DVec3::new(17.0, 11.0, 7.0))).sin()
                + 0.7 * (p.dot(DVec3::new(-9.0, 23.0, 15.0))).cos()
                + 0.5 * (p.dot(DVec3::new(31.0, -13.0, 19.0))).sin();
            pixels.push(if v > 0.0 { 255 } else { 0 });
        }
    }
    SphereTemplate::new(w, h, pixels)
}
fn render_template(
    c: &Config,
    t: &SphereTemplate,
    q: DQuat,
    time: f64,
) -> fictrac_core::Result<GrayFrame> {
    let g = Geometry::new(c.clone())?;
    let basis = DQuat::from_rotation_arc(DVec3::from_array(c.sphere.centre), DVec3::Z);
    let mut pixels = vec![0.12; c.width * c.height];
    for y in 0..c.height {
        for x in 0..c.width {
            if let Some(p) = g.surface(x as f64, y as f64) {
                pixels[y * c.width + x] =
                    if t.pixels[index(basis * (q.inverse() * p), t.width, t.height)] == 255 {
                        0.95
                    } else {
                        0.05
                    };
            }
        }
    }
    GrayFrame::new(c.width, c.height, pixels, time)
}
#[test]
fn imported_template_relocalizes_large_unknown_pose_without_inventing_motion() -> Result {
    let c = config(true);
    let t = known_template()?;
    let expected = DQuat::from_scaled_axis(DVec3::new(1.1, -0.9, 0.5));
    let mut tracker = Tracker::new(c.clone())?;
    tracker.load_sphere_template(t.clone())?;
    let row = tracker.process(render_template(&c, &t, expected, 0.0)?)?;
    let mut repeat = Tracker::new(c.clone())?;
    repeat.load_sphere_template(t.clone())?;
    let repeated = repeat.process(render_template(&c, &t, expected, 0.0)?)?;
    assert_eq!(row.absolute_camera, repeated.absolute_camera);
    assert_eq!(row.error, repeated.error);
    let actual = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
    let error = (actual * expected.inverse()).to_scaled_axis().length();
    assert!(row.accepted, "{}: {}", row.status, row.error);
    assert_eq!(row.status, "relocalized");
    assert!(
        error < 0.08,
        "pose error {error}, score {}, found {:?}",
        row.error,
        row.absolute_camera
    );
    assert_eq!(row.delta_camera, [0.0; 3]);
    assert_eq!(row.position, [0.0; 2]);
    assert_eq!(row.integrated, [0.0; 2]);
    let delta = DVec3::new(0.008, -0.011, 0.005);
    let next = tracker.process(render_template(
        &c,
        &t,
        DQuat::from_scaled_axis(delta) * expected,
        10.0,
    )?)?;
    assert!(next.accepted, "next score {}", next.error);
    assert_eq!(next.status, "tracked");
    assert!(
        (DVec3::from_array(next.delta_camera) - delta).length() < 0.008,
        "delta {:?}",
        next.delta_camera
    );
    let before_gap = next.clone();
    let before_map = tracker.sphere_template();
    let lost = tracker.process(GrayFrame::new(
        c.width,
        c.height,
        vec![0.5; c.width * c.height],
        20.0,
    )?)?;
    assert!(!lost.accepted);
    assert_eq!(lost.position, before_gap.position);
    assert_eq!(lost.integrated, before_gap.integrated);
    assert_eq!(lost.delta_ms, 10.0);
    assert_eq!(tracker.sphere_template(), before_map);
    let recovered_pose = DQuat::from_scaled_axis(DVec3::new(-1.3, 0.6, -0.7));
    let recovered = tracker.process(render_template(&c, &t, recovered_pose, 30.0)?)?;
    assert!(recovered.accepted, "recovery score {}", recovered.error);
    assert_eq!(recovered.status, "relocalized");
    assert_eq!(recovered.delta_camera, [0.0; 3]);
    assert_eq!(recovered.position, before_gap.position);
    assert_eq!(recovered.integrated, before_gap.integrated);
    assert_eq!(recovered.delta_ms, 20.0);
    let actual = DQuat::from_scaled_axis(DVec3::from_array(recovered.absolute_camera));
    assert!(
        (actual * recovered_pose.inverse())
            .to_scaled_axis()
            .length()
            < 0.08
    );
    let retained = tracker.sphere_template();
    tracker.reset_tracking();
    assert_eq!(tracker.sphere_template(), retained);
    let after_reset = tracker.process(render_template(&c, &t, recovered_pose, 40.0)?)?;
    assert!(after_reset.accepted);
    assert_eq!(after_reset.status, "relocalized");
    assert_eq!(after_reset.frame, 4);
    assert_eq!(after_reset.sequence, 0);
    assert_eq!(after_reset.delta_ms, 10.0);
    assert_eq!(after_reset.integrated, before_gap.integrated);
    assert_eq!(after_reset.position, [0.0; 2]);
    Ok(())
}
#[test]
fn bad_frames_do_not_contaminate_map_and_reset_only_after_threshold() -> Result {
    let mut c = config(false);
    if let Some(m) = &mut c.spherical_map {
        m.max_bad_frames = 1;
    }
    let mut tracker = Tracker::new(c.clone())?;
    tracker.process(synthetic::render(&c, [0.0; 3], 0.0)?)?;
    let initial = tracker.sphere_template().ok_or("map missing")?;
    for i in 1..=2 {
        let row = tracker.process(GrayFrame::new(
            c.width,
            c.height,
            vec![0.5; c.width * c.height],
            i as f64,
        )?)?;
        assert!(!row.accepted);
        assert_eq!(row.delta_camera, [0.0; 3]);
        if i == 1 {
            assert_eq!(tracker.sphere_template(), Some(initial.clone()));
        }
    }
    let cleared = tracker.sphere_template().ok_or("map missing")?;
    assert!(cleared.pixels.iter().all(|p| *p == 128));
    let row = tracker.process(synthetic::render(&c, [0.0; 3], 3.0)?)?;
    assert_eq!(row.frame, 3);
    assert_eq!(row.sequence, 0);
    assert_eq!(row.delta_ms, 3.0);
    assert_eq!(row.status, "initializing");
    assert!(row.accepted);
    Ok(())
}
#[test]
fn manual_reset_retains_integrated_displacement_and_counter() -> Result {
    let c = Config::default();
    let mut tracker = Tracker::new(c.clone())?;
    tracker.process(synthetic::render(&c, [0.0; 3], 0.0)?)?;
    let moving = tracker.process(synthetic::render(&c, [0.008, -0.011, 0.005], 10.0)?)?;
    assert!(moving.accepted);
    assert!(moving.integrated[0].abs() > 0.001);
    tracker.reset_tracking();
    let row = tracker.process(synthetic::render(&c, [0.008, -0.011, 0.005], 20.0)?)?;
    assert_eq!(row.frame, 3);
    assert_eq!(row.sequence, 0);
    assert_eq!(row.integrated, moving.integrated);
    assert_eq!(row.position, [0.0; 2]);
    assert_eq!(row.heading, 0.0);
    assert_eq!(row.absolute_camera, [0.0; 3]);
    Ok(())
}
#[test]
fn invalid_import_and_recovery_settings_do_not_mutate_state() -> Result {
    let c = config(true);
    let mut tracker = Tracker::new(c.clone())?;
    tracker.process(synthetic::render(&c, [0.0; 3], 0.0)?)?;
    let initial = tracker.sphere_template();
    let wrong = SphereTemplate::new(30, 15, vec![0; 450])?;
    assert!(tracker.load_sphere_template(wrong).is_err());
    assert_eq!(tracker.sphere_template(), initial);
    assert!(SphereTemplate::new(30, usize::MAX, vec![]).is_err());
    assert!(SphereTemplate::new(180, 90, vec![128; 180 * 90]).is_err());
    for budget in [0, 255, usize::MAX] {
        let mut invalid = c.clone();
        if let Some(m) = &mut invalid.spherical_map {
            m.global_evaluations = budget;
        }
        assert!(Tracker::new(invalid).is_err());
    }
    Ok(())
}

#[test]
fn imported_map_recovery_preserves_off_axis_legacy_coordinates() -> Result {
    let mut c = config(true);
    c.sphere.centre = DVec3::new(-0.10, 0.06, 1.0).normalize().to_array();
    c.sphere.angular_radius = 0.22;
    let t = known_template()?;
    let physical = DQuat::from_scaled_axis(DVec3::new(0.4, 0.9, -0.6));
    let basis = DQuat::from_rotation_arc(DVec3::from_array(c.sphere.centre), DVec3::Z);
    let mut tracker = Tracker::new(c.clone())?;
    tracker.load_sphere_template(t.clone())?;
    let row = tracker.process(render_template(&c, &t, physical, 0.0)?)?;
    assert!(row.accepted, "score {}", row.error);
    assert_eq!(row.status, "relocalized");
    let expected = basis * physical * basis.inverse();
    let found = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
    let error = 2.0 * found.dot(expected).abs().clamp(0.0, 1.0).acos();
    assert!(error < 0.08, "ROI pose error {error}");
    assert_eq!(row.delta_camera, [0.0; 3]);
    Ok(())
}

#[test]
fn modern_rows_keep_processed_frame_intervals_after_rejections() -> Result {
    let c = Config::default();
    let mut tracker = Tracker::new(c.clone())?;
    tracker.process(synthetic::render(&c, [0.0; 3], 0.0)?)?;
    let lost = tracker.process(GrayFrame::new(
        c.width,
        c.height,
        vec![0.5; c.width * c.height],
        10.0,
    )?)?;
    assert!(!lost.accepted);
    assert_eq!(lost.delta_ms, 10.0);
    tracker.process(synthetic::render(&c, [0.0; 3], 20.0)?)?;
    let recovered = tracker.process(synthetic::render(&c, [0.0; 3], 30.0)?)?;
    assert!(recovered.accepted);
    assert_eq!(recovered.delta_ms, 10.0);
    Ok(())
}

#[test]
fn local_map_search_preserves_known_motion_trajectory_over_120_frames() -> Result {
    let mut c = config(false);
    if let Some(map) = &mut c.spherical_map {
        map.max_evaluations = 50;
    }
    let template = known_template()?;
    let delta = DVec3::new(0.004, -0.007, 0.003);
    let mut tracker = Tracker::new(c.clone())?;
    let mut expected_position = [0.0, 0.0];
    let mut path_squared = 0.0;
    let mut heading_squared = 0.0;
    let mut rotation_squared = DVec3::ZERO;
    for frame in 0..120 {
        let pose = DQuat::from_scaled_axis(delta * frame as f64);
        let row = tracker.process(render_template(&c, &template, pose, frame as f64 * 10.0)?)?;
        assert!(row.accepted, "frame={frame}, score={}", row.error);
        if frame == 0 {
            continue;
        }
        let prior_heading = -delta.z * (frame - 1) as f64;
        for substep in 0..4 {
            let angle = prior_heading - delta.z * (substep as f64 + 0.5) / 4.0;
            let (s, cos) = angle.sin_cos();
            expected_position[0] += (cos * delta.y + s * delta.x) / 4.0;
            expected_position[1] += (s * delta.y - cos * delta.x) / 4.0;
        }
        path_squared += (row.position[0] - expected_position[0]).powi(2)
            + (row.position[1] - expected_position[1]).powi(2);
        let angle = row.heading + delta.z * frame as f64;
        heading_squared += angle.sin().atan2(angle.cos()).powi(2);
        let error = DVec3::from_array(row.delta_camera) - delta;
        rotation_squared += error * error;
    }
    let path_rmse = (path_squared / 119.0).sqrt();
    let heading_rmse = (heading_squared / 119.0).sqrt();
    let rotation_rmse = (rotation_squared / 119.0).to_array().map(f64::sqrt);
    assert!(path_rmse < 0.08, "known-motion trajectory RMSE {path_rmse}");
    assert!(
        heading_rmse < 0.08,
        "known-motion heading RMSE {heading_rmse}"
    );
    assert!(
        rotation_rmse.iter().all(|v| *v < 0.008),
        "known-motion delta RMSE {rotation_rmse:?}"
    );
    Ok(())
}

#[test]
fn resilient_atlas_survives_240_occluder_frames_and_reacquires_without_path_jump() -> Result {
    let mut c = config(false);
    if let Some(m) = &mut c.spherical_map {
        m.resilient = true;
        m.max_evaluations = 50;
        m.max_bad_frames = 2;
        // Exercise the lowest accepted global cap as well as long occlusion.
        m.global_evaluations = 256;
    }
    let texture = known_template()?;
    let delta = DVec3::new(0.004, -0.007, 0.003);
    let mut tracker = Tracker::new(c.clone())?;
    let mut last = None;
    for i in 0..400 {
        let row = tracker.process(render_template(
            &c,
            &texture,
            DQuat::from_scaled_axis(delta * i as f64),
            i as f64 * 10.0,
        )?)?;
        assert!(
            row.accepted,
            "clean frame {i}: {} {}",
            row.status, row.error
        );
        last = Some(row);
    }
    let last = last.ok_or("no learned frame")?;
    let atlas = tracker.sphere_template().ok_or("missing learned atlas")?;
    for i in 400..640 {
        let pixels = (0..c.width * c.height)
            .map(|index| {
                if i < 480 {
                    0.5
                } else if i < 560 {
                    0.1 + 0.8 * (index % c.width) as f32 / c.width as f32
                } else {
                    let v = (index as u64 * 73 + i as u64 * 193).wrapping_mul(6364136223846793005);
                    (v >> 56) as f32 / 255.0
                }
            })
            .collect();
        let row = tracker.process(GrayFrame::new(c.width, c.height, pixels, i as f64 * 10.0)?)?;
        assert!(!row.accepted, "occluder frame {i} accepted: {}", row.error);
        assert_eq!(row.delta_camera, [0.0; 3]);
        assert_eq!(row.position, last.position);
        assert_eq!(row.integrated, last.integrated);
        assert_eq!(
            tracker.sphere_template().as_ref(),
            Some(&atlas),
            "learned map changed during occlusion"
        );
    }
    let mut recovered = None;
    for j in 0..120 {
        let expected = DQuat::from_scaled_axis(delta * (30 + j) as f64);
        let row = tracker.process(render_template(
            &c,
            &texture,
            expected,
            (640 + j) as f64 * 10.0,
        )?)?;
        if row.accepted {
            assert_eq!(row.status, "relocalized");
            assert_eq!(row.delta_camera, [0.0; 3]);
            assert_eq!(row.position, last.position);
            assert_eq!(row.integrated, last.integrated);
            let found = DQuat::from_scaled_axis(DVec3::from_array(row.absolute_camera));
            let error = (found * expected.inverse()).to_scaled_axis().length();
            assert!(error < 0.08, "absolute reacquisition error {error}");
            recovered = Some(j);
            break;
        }
        assert_eq!(tracker.sphere_template().as_ref(), Some(&atlas));
    }
    let j = recovered.ok_or("did not reacquire the learned ball within 120 frames")?;
    let row = tracker.process(render_template(
        &c,
        &texture,
        DQuat::from_scaled_axis(delta * (31 + j) as f64),
        (641 + j) as f64 * 10.0,
    )?)?;
    assert!(row.accepted);
    assert_eq!(row.status, "tracked");
    assert!((DVec3::from_array(row.delta_camera) - delta).length() < 0.012);
    Ok(())
}
