//! Learned-atlas occlusion regression using the unchanged, decoded real sample.
use fictrac_core::{Geometry, GrayFrame, TrackRow, Tracker};
use fictrac_io::{VideoSource, load_config_text};
use std::path::PathBuf;

type Result = std::result::Result<(), Box<dyn std::error::Error>>;
const WARMUP: usize = 900;
const OCCLUDED: usize = 240;
const AFTER: usize = 180;

fn sample_index(frame: usize, count: usize) -> usize {
    let index = frame % ((count - 1) * 2);
    if index < count {
        index
    } else {
        (count - 1) * 2 - index
    }
}
fn translated_path(row: &TrackRow, origin: &TrackRow, angle: f64) -> [f64; 2] {
    let x = row.position[0] - origin.position[0];
    let y = row.position[1] - origin.position[1];
    let (s, c) = angle.sin_cos();
    [c * x - s * y, s * x + c * y]
}

#[test]
fn looping_real_video_rejects_240_occluders_and_resumes_without_corrupting_atlas() -> Result {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut video = VideoSource::open(&root.join("examples/sample.mp4"))?;
    video.enable_color();
    let mut frames = vec![];
    while let Some(frame) = video.next_tracking_frame(None)? {
        frames.push(frame);
    }
    assert_eq!(frames.len(), 300);
    let text = std::fs::read_to_string(root.join("examples/benchmark/config.txt"))?;
    let mut config = load_config_text(&text, 384, 288)?.config;
    if let Some(map) = &mut config.spherical_map {
        map.resilient = true;
        // Prove legacy's finite reset policy cannot clear a protected atlas.
        map.max_bad_frames = 2;
    }
    let mut control = Tracker::new(config.clone())?;
    let mut interrupted = Tracker::new(config.clone())?;
    let mut last = None;
    for i in 0..WARMUP {
        let frame = frames[sample_index(i, frames.len())]
            .clone()
            .with_timestamp(i as f64 * 10.0)?;
        let reference = control.process(frame.clone())?;
        let row = interrupted.process(frame)?;
        assert!(
            reference.accepted && row.accepted,
            "clean warmup {i}: {} {}",
            row.status,
            row.error
        );
        assert_eq!(reference.delta_camera, row.delta_camera);
        last = Some(row);
    }
    let last = last.ok_or("no learned video frame")?;
    let atlas = interrupted
        .sphere_template()
        .ok_or("missing learned atlas")?;
    for i in WARMUP..WARMUP + OCCLUDED {
        control.process(
            frames[sample_index(i, frames.len())]
                .clone()
                .with_timestamp(i as f64 * 10.0)?,
        )?;
        let phase = i - WARMUP;
        let pixels = (0..config.width * config.height)
            .map(|index| {
                if phase < 80 {
                    0.5
                } else if phase < 160 {
                    0.1 + 0.8 * (index % config.width) as f32 / config.width as f32
                } else {
                    let v = (index as u64 * 73 + i as u64 * 193).wrapping_mul(6364136223846793005);
                    (v >> 56) as f32 / 255.0
                }
            })
            .collect();
        let row = interrupted.process(GrayFrame::new(
            config.width,
            config.height,
            pixels,
            i as f64 * 10.0,
        )?)?;
        assert!(
            !row.accepted,
            "occluder {phase}: {} {}",
            row.status, row.error
        );
        assert_eq!(row.delta_camera, [0.0; 3]);
        assert_eq!(row.position, last.position);
        assert_eq!(row.integrated, last.integrated);
        assert_eq!(interrupted.sphere_template().as_ref(), Some(&atlas));
    }
    let mut origins = None;
    let mut recovery_frame = None;
    let mut count = 0usize;
    let mut path_squared = 0.0;
    let mut heading_squared = 0.0;
    let mut delta_squared = [0.0; 3];
    for j in 0..AFTER {
        let i = WARMUP + OCCLUDED + j;
        let frame = frames[sample_index(i, frames.len())]
            .clone()
            .with_timestamp(i as f64 * 10.0)?;
        let reference = control.process(frame.clone())?;
        let row = interrupted.process(frame)?;
        assert!(
            reference.accepted,
            "clean control after gap {j}: {}",
            reference.error
        );
        if origins.is_none() {
            if !row.accepted {
                assert_eq!(interrupted.sphere_template().as_ref(), Some(&atlas));
                continue;
            }
            assert_eq!(row.status, "relocalized");
            assert_eq!(row.delta_camera, [0.0; 3]);
            assert_eq!(row.position, last.position);
            assert_eq!(row.integrated, last.integrated);
            recovery_frame = Some(j);
            origins = Some((reference, row));
            continue;
        }
        assert!(
            row.accepted,
            "post-recovery frame {j}: {} {}",
            row.status, row.error
        );
        let (reference_origin, actual_origin) =
            origins.as_ref().ok_or("missing recovery origin")?;
        // The unknown interval is not fabricated. Align only the unavailable
        // translation/heading offsets when comparing the measured suffix.
        let angle = actual_origin.heading - reference_origin.heading;
        let expected = translated_path(&reference, reference_origin, angle);
        let actual = translated_path(&row, actual_origin, 0.0);
        path_squared += (actual[0] - expected[0]).powi(2) + (actual[1] - expected[1]).powi(2);
        let heading = row.heading - reference.heading - angle;
        heading_squared += heading.sin().atan2(heading.cos()).powi(2);
        for (axis, squared) in delta_squared.iter_mut().enumerate() {
            *squared += (row.delta_camera[axis] - reference.delta_camera[axis]).powi(2);
        }
        count += 1;
    }
    let recovered = recovery_frame.ok_or("did not reacquire looping sample video")?;
    assert!(
        recovered < 120,
        "reacquisition took {recovered} visible frames"
    );
    assert!(count >= 60, "not enough measured frames after recovery");
    let path_rmse = (path_squared / count as f64).sqrt();
    let heading_rmse = (heading_squared / count as f64).sqrt();
    let delta_rmse = delta_squared.map(|v| (v / count as f64).sqrt());
    assert!(path_rmse < 0.075, "aligned suffix path RMSE {path_rmse}");
    assert!(
        heading_rmse < 0.025,
        "aligned suffix heading RMSE {heading_rmse}"
    );
    assert!(
        delta_rmse.iter().all(|v| *v < 0.020),
        "suffix delta RMSE {delta_rmse:?}"
    );
    // A hand usually obscures only part of the ROI. Cover approximately 70%
    // of its actual visible/exclusion-masked sphere pixels, then remove it.
    let geometry = Geometry::new(config.clone())?;
    let mut visible_x = vec![];
    for y in 0..config.height {
        for x in 0..config.width {
            if geometry.usable(x as f64, y as f64) && geometry.surface(x as f64, y as f64).is_some()
            {
                visible_x.push(x);
            }
        }
    }
    visible_x.sort_unstable();
    let cutoff = *visible_x
        .get(visible_x.len() * 7 / 10)
        .ok_or("no visible ball pixels")?;
    let covered =
        visible_x.iter().filter(|x| **x <= cutoff).count() as f64 / visible_x.len() as f64;
    assert!(
        (0.65..0.75).contains(&covered),
        "partial obstruction coverage {covered}"
    );
    let start = WARMUP + OCCLUDED + AFTER;
    let mut rejected_partial = 0;
    for i in start..start + 120 {
        let frame = frames[sample_index(i, frames.len())]
            .clone()
            .with_timestamp(i as f64 * 10.0)?;
        let reference = control.process(frame.clone())?;
        let before = interrupted.sphere_template();
        let mut pixels = frame.pixels().to_vec();
        for (index, pixel) in pixels.iter_mut().enumerate() {
            if index % config.width <= cutoff {
                *pixel = if i < start + 60 {
                    0.5
                } else {
                    0.1 + 0.8 * (index / config.width) as f32 / config.height as f32
                };
            }
        }
        let row = interrupted.process(GrayFrame::new(
            config.width,
            config.height,
            pixels,
            i as f64 * 10.0,
        )?)?;
        if !row.accepted {
            rejected_partial += 1;
            assert_eq!(
                interrupted.sphere_template(),
                before,
                "rejected partial frame taught the atlas"
            );
            assert_eq!(row.delta_camera, [0.0; 3]);
        } else if row.status == "tracked" {
            for axis in 0..3 {
                assert!(
                    (row.delta_camera[axis] - reference.delta_camera[axis]).abs() < 0.020,
                    "partial accepted inaccurate motion at frame {i}"
                );
            }
        }
    }
    assert!(
        rejected_partial > 0,
        "partial fixture never exercised rejection protection"
    );
    let mut measured = 0;
    let mut partial_delta_squared = [0.0; 3];
    for i in start + 120..start + 300 {
        let frame = frames[sample_index(i, frames.len())]
            .clone()
            .with_timestamp(i as f64 * 10.0)?;
        let reference = control.process(frame.clone())?;
        let row = interrupted.process(frame)?;
        if row.accepted && row.status == "tracked" {
            for (axis, squared) in partial_delta_squared.iter_mut().enumerate() {
                *squared += (row.delta_camera[axis] - reference.delta_camera[axis]).powi(2);
            }
            measured += 1;
        } else if row.accepted {
            assert_eq!(row.delta_camera, [0.0; 3]);
        }
    }
    assert!(
        measured >= 60,
        "only {measured} usable suffix frames after partial obstruction"
    );
    let partial_delta_rmse = partial_delta_squared.map(|v| (v / measured as f64).sqrt());
    assert!(
        partial_delta_rmse.iter().all(|v| *v < 0.020),
        "post-partial delta RMSE {partial_delta_rmse:?}"
    );
    Ok(())
}
