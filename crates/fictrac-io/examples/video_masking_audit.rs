//! Synthetic occluders on decoded real images; unobstructed replay is a control, not truth.
use fictrac_core::{Geometry, GrayFrame, Tracker};
use fictrac_io::{VideoSource, load_config, write_dat};
use serde_json::json;
use std::{fs, io::Write, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: video_masking_audit VIDEO CONFIG NEW_DIRECTORY".into());
    }
    let output = PathBuf::from(&args[2]);
    fs::create_dir_all(output.parent().ok_or("output needs a parent")?)?;
    fs::create_dir(&output)?;
    let mut source = VideoSource::open(args[0].as_ref())?;
    source.enable_color();
    let config = load_config(args[1].as_ref(), source.info.width, source.info.height)?.config;
    let channel = config.spherical_map.as_ref().and_then(|m| m.channel);
    let mut frames = Vec::new();
    let mut bytes = 0usize;
    while let Some(frame) = source.next_tracking_frame(channel)? {
        bytes = bytes
            .checked_add(frame.pixels().len() * 4)
            .ok_or("fixture allocation overflow")?;
        if bytes > 512 * 1024 * 1024 {
            return Err("decoded audit fixture exceeds 512MiB".into());
        }
        frames.push(frame);
    }
    if frames.len() < 2 {
        return Err("need at least two frames".into());
    }
    let g = Geometry::new(config.clone())?;
    let mut visible = Vec::new();
    for x in 0..config.width {
        for y in 0..config.height {
            if g.usable(x as f64, y as f64) && g.surface(x as f64, y as f64).is_some() {
                visible.push(y * config.width + x);
            }
        }
    }
    let index = |i: usize| {
        let cycle = i % (2 * (frames.len() - 1));
        if cycle < frames.len() {
            cycle
        } else {
            2 * (frames.len() - 1) - cycle
        }
    };
    let mut reports = Vec::new();
    for warmup in [10usize, 40, 120] {
        for percent in [10usize, 25, 50, 70] {
            for pattern in 0..6 {
                let id = reports.len();
                let mut control = Tracker::new(config.clone())?;
                let mut masked = Tracker::new(config.clone())?;
                let mut data = std::io::BufWriter::new(fs::File::create(
                    output.join(format!("case-{id:02}.dat")),
                )?);
                let mut quality = std::io::BufWriter::new(fs::File::create(
                    output.join(format!("case-{id:02}-quality.csv")),
                )?);
                writeln!(quality, "frame,phase,accepted,status,fit_error")?;
                let mut mask_measured = 0usize;
                let mut mask_inaccurate = 0usize;
                let mut clear_measured = 0usize;
                let mut clear_squared = [0.; 3];
                let mut rejected_mutations = 0usize;
                let mut first = None;
                for i in 0..warmup + 240 {
                    let clean = frames[index(i)].clone().with_timestamp(i as f64 * 10.)?;
                    let reference = control.process(clean.clone())?;
                    let is_masked = i >= warmup && i < warmup + 60;
                    let image = if is_masked {
                        let mut pixels = clean.pixels().to_vec();
                        let target = visible.len() * percent / 100;
                        let phase = i - warmup;
                        let count = if pattern == 5 {
                            target * phase.min(30) / 30
                        } else {
                            target
                        };
                        let offset = if pattern == 4 {
                            phase * (visible.len() - target) / 59
                        } else {
                            0
                        };
                        for &site in visible.iter().skip(offset).take(count) {
                            pixels[site] = match pattern {
                                0 | 4 => 0.,
                                1 => 1.,
                                2 => {
                                    0.1 + 0.8 * (site / config.width) as f32 / config.height as f32
                                }
                                _ => {
                                    let v = (site as u64 * 73).wrapping_mul(6364136223846793005);
                                    (v >> 56) as f32 / 255.
                                }
                            };
                        }
                        GrayFrame::new(config.width, config.height, pixels, clean.timestamp_ms())?
                    } else {
                        clean
                    };
                    let before = masked.sphere_template();
                    let row = masked.process(image)?;
                    if !row.accepted && before != masked.sphere_template() {
                        rejected_mutations += 1;
                    }
                    if row.status == "tracked" && reference.status == "tracked" {
                        let error = std::array::from_fn::<_, 3, _>(|axis| {
                            row.delta_camera[axis] - reference.delta_camera[axis]
                        });
                        if is_masked {
                            mask_measured += 1;
                            mask_inaccurate += usize::from(error.iter().any(|x| x.abs() >= 0.020));
                        }
                        if i >= warmup + 60 {
                            clear_measured += 1;
                            first.get_or_insert(i - warmup - 60);
                            for axis in 0..3 {
                                clear_squared[axis] += error[axis].powi(2);
                            }
                        }
                    }
                    write_dat(&mut data, &row)?;
                    writeln!(
                        quality,
                        "{i},{},{},{},{}",
                        if i < warmup {
                            "warmup"
                        } else if is_masked {
                            "masked"
                        } else {
                            "clear"
                        },
                        row.accepted,
                        row.status,
                        row.error
                    )?;
                }
                let rms = (clear_measured > 0)
                    .then(|| clear_squared.map(|x| (x / clear_measured as f64).sqrt()));
                reports.push(json!({"warmup":warmup,"covered_percent":percent,"pattern":pattern,"masked_measured":mask_measured,"masked_inaccurate_vs_control":mask_inaccurate,"clear_measured":clear_measured,"clear_component_rmse_vs_control":rms,"first_measured_clear_frame":first,"rejected_atlas_mutations":rejected_mutations}));
                eprintln!(
                    "case{id}: warm{warmup} cover{percent} pattern{pattern} masked{mask_measured} inaccurate{mask_inaccurate} clear{clear_measured} rms{rms:?}"
                );
                fs::write(
                    output.join("report.json"),
                    serde_json::to_string_pretty(
                        &json!({"input":args[0],"decoded_frames":frames.len(),"config":config,"mask_frames":60,"clear_frames":180,"pattern_names":["black","white","gradient","fixed_unrelated_texture","moving_black_panel","growing_unrelated_texture"],"scope":"Synthetic panels on decoded real video. Identically timed unoccluded replay supplies comparison increments, not physical ground truth. Missing rows stay explicit; no accuracy inferred from acceptance. Growing masks reach their target coverage after30 images.","cases":reports}),
                    )?,
                )?;
            }
        }
    }
    Ok(())
}
