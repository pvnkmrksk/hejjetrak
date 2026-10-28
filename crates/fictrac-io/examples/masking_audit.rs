//! Serial known-motion audit. This development driver never decodes through a subprocess.
#[path = "../../fictrac-core/tests/support/occlusion.rs"]
mod occlusion;
use occlusion::{Case, Scene, TestResult};
use std::{fs, io::Write, path::PathBuf, time::Instant};

fn main() -> TestResult<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(1..=3).contains(&args.len()) {
        return Err(
            "usage: masking_audit NEW_DIRECTORY [ERROR_FLOOR=10000] [smoke|full|return|advanced]"
                .into(),
        );
    }
    let output = PathBuf::from(&args[0]);
    fs::create_dir_all(output.parent().ok_or("output needs a parent directory")?)?;
    fs::create_dir(&output)?;
    let floor: f64 = args.get(1).map_or(Ok(10000.), |s| s.parse())?;
    let smoke = args.get(2).is_some_and(|s| s == "smoke");
    let return_to_known = args.get(2).is_some_and(|s| s == "return");
    let advanced = args.get(2).is_some_and(|s| s == "advanced");
    if args
        .get(2)
        .is_some_and(|s| !matches!(s.as_str(), "smoke" | "full" | "return" | "advanced"))
    {
        return Err("audit mode must be smoke, full, return or advanced".into());
    }
    let scene = Scene::new(floor)?;
    let mut rows = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("rows.csv"))?;
    writeln!(
        rows,
        "case,frame,phase,true_x,true_y,true_z,measured_x,measured_y,measured_z,accepted,status,fit_error"
    )?;
    let mut reports = Vec::new();
    let started = Instant::now();
    let stages = if return_to_known || advanced {
        vec![(40, true), (80, false), (400, false)]
    } else {
        vec![(0, true), (40, true), (80, false), (400, false)]
    };
    for (warmup, stationary) in stages {
        for acceleration in [false, true] {
            for covered_percent in if smoke || return_to_known || advanced {
                vec![25, 70]
            } else {
                vec![0, 10, 25, 50, 70, 100]
            } {
                for pattern in if advanced {
                    vec![4, 5]
                } else {
                    (0..4).collect()
                } {
                    if covered_percent == 0 && pattern != 0 {
                        continue;
                    }
                    let case = Case {
                        warmup,
                        stationary,
                        covered_percent,
                        pattern,
                        acceleration,
                        return_to_known,
                    };
                    let id = reports.len();
                    let warm_angle = if stationary {
                        0.
                    } else {
                        0.02 * warmup.saturating_sub(1) as f64
                    };
                    let mask_angle: f64 = (0..=30)
                        .map(|j| {
                            if acceleration {
                                0.012 + 0.053 * ((j as f64 - 10.) / 15.).clamp(0., 1.)
                            } else {
                                0.012
                            }
                        })
                        .sum();
                    let pose = glam::DQuat::from_scaled_axis(
                        glam::DVec3::new(1., -0.3, 0.2).normalize() * (warm_angle + mask_angle),
                    );
                    let preview = scene.render(pose, warmup + 30, Some(case))?;
                    let mut image =
                        fs::File::create(output.join(format!("case-{id:03}-input.pgm")))?;
                    writeln!(image, "P5\n{} {}\n255", preview.width(), preview.height())?;
                    image.write_all(
                        &preview
                            .pixels()
                            .iter()
                            .map(|p| (p * 255.).round() as u8)
                            .collect::<Vec<_>>(),
                    )?;
                    let report = scene.audit(case, |frame, phase, delta, row| {
                        writeln!(
                            rows,
                            "{id},{frame},{phase},{},{},{},{},{},{},{},{},{}",
                            delta.x,
                            delta.y,
                            delta.z,
                            row.delta_camera[0],
                            row.delta_camera[1],
                            row.delta_camera[2],
                            row.accepted,
                            row.status,
                            row.error
                        )?;
                        Ok(())
                    })?;
                    eprintln!(
                        "case {id}: {case:?} mask={}/{} inaccurate, clear={}/180 ({:?}) atlas_wrong={:.3}",
                        report.masked_inaccurate,
                        report.masked_measured,
                        report.clear_measured,
                        report.stable_clear_frame,
                        report.atlas_wrong_fraction
                    );
                    reports.push(report);
                    fs::write(
                        output.join("report.json"),
                        serde_json::to_string_pretty(
                            &serde_json::json!({"config":scene.config,"pattern_names":["black","white","gradient","fixed_unrelated_texture","moving_black_panel","growing_unrelated_texture"],"mask_frames":60,"clear_frames":180,"gate_note":"Diagnostic inaccurate-row threshold .020 rad/component; existing regression gates remain unchanged. Missing rows never count as correct motion. Growing masks reach their target coverage after30 images.","cases":reports,"wall_seconds":started.elapsed().as_secs_f64()}),
                        )?,
                    )?;
                }
            }
        }
    }
    Ok(())
}
