use clap::{Parser, Subcommand};
use fictrac_core::{Config, Geometry, Tracker, synthetic};
use fictrac_io::{
    VideoSource, load_config, load_source_hint, write_dat, write_path_svg, write_y4m_frame,
    write_y4m_header,
};
use serde::Serialize;
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(
    name = "hejjetrac-cli",
    version,
    about = "HejjéTrac: native Rust tracking, FicTrac compatibility and agent control"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Print the machine-readable local control protocol and command discovery.
    Schema,
    /// Serve deterministic agent commands as JSONL on stdin/stdout.
    Agent,
    /// Serve the same engine to MCP clients over local stdio JSON-RPC.
    Mcp,
    /// Read or edit legacy configuration fields without normalizing other text.
    Config {
        #[arg(long)]
        config: PathBuf,
        /// Set key=value; repeat for an atomic batch of document edits.
        #[arg(long = "set")]
        fields: Vec<String>,
        /// New output file. With no output, print the edited document to stdout.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Compare aligned 25-column data against a reference, excluding the initial clock anomaly.
    Compare {
        #[arg(long)]
        reference: PathBuf,
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        report: Option<PathBuf>,
        #[arg(long, default_value_t = 0.075)]
        max_path_rmse: f64,
        #[arg(long, default_value_t = 0.025)]
        max_heading_rmse: f64,
        #[arg(long, default_value_t = 0.020)]
        max_rotation_rmse: f64,
    },
    /// Track an H.264 MP4/MOV or mono/420 Y4M using calibrated sphere geometry.
    Track {
        #[arg(short, long)]
        config: PathBuf,
        /// Overrides source from config. Relative paths are relative to current directory.
        #[arg(short, long)]
        input: Option<PathBuf>,
        /// A NEW directory; existing results are never overwritten.
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        max_frames: Option<usize>,
    },
    /// Print stream metadata; optionally decode every frame and save first luma image.
    Inspect {
        input: PathBuf,
        #[arg(long)]
        decode: bool,
        #[arg(long)]
        preview: Option<PathBuf>,
    },
    /// Fit a sphere to three or more rim pixels and write a JSON calibration.
    Calibrate {
        #[arg(long)]
        width: usize,
        #[arg(long)]
        height: usize,
        #[arg(long)]
        vfov: f64,
        /// JSON x/y pairs, e.g. '[[63,171],[81,145],[106,135],[150,160]]'.
        #[arg(long)]
        points: String,
        /// JSON Rodrigues vector, radians, camera -> lab.
        #[arg(long, default_value = "[0,0,0]")]
        lab_rotation: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Generate a known-motion video, track it, and check rotation accuracy.
    Demo {
        #[arg(short, long, default_value = "demo-run")]
        output: PathBuf,
        #[arg(long, default_value_t = 120)]
        frames: usize,
    },
}
#[derive(Serialize)]
struct Summary {
    version: &'static str,
    input: String,
    video: fictrac_io::VideoInfo,
    effective_config: Config,
    frames: usize,
    tracked: usize,
    relocalized: usize,
    rejected: usize,
    initializing: usize,
    mean_error: f64,
    max_error: f64,
    wall_seconds: f64,
    end_to_end_fps: f64,
    tracking_fps: f64,
    input_duration_ms: f64,
    final_position: [f64; 2],
    final_heading: f64,
    estimator: &'static str,
    error_metric: &'static str,
    warnings: Vec<String>,
}
fn new_file(path: impl AsRef<Path>) -> Result<BufWriter<File>> {
    Ok(BufWriter::new(
        OpenOptions::new().write(true).create_new(true).open(path)?,
    ))
}
fn create_run_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(path)?;
    Ok(())
}
fn track(
    input: &Path,
    config_path: &Path,
    output: &Path,
    max_frames: Option<usize>,
) -> Result<Summary> {
    let wall = Instant::now();
    if max_frames.is_some_and(|n| n < 2) {
        return Err("--max-frames must be at least 2".into());
    }
    let mut source = VideoSource::open(input)?;
    let loaded = load_config(config_path, source.info.width, source.info.height)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {warning}");
    }
    let text = fs::read_to_string(config_path)?;
    let document = if text.trim_start().starts_with('{') {
        fictrac_io::ConfigDocument::parse(String::new())?
    } else {
        fictrac_io::ConfigDocument::parse(text)?
    };
    let requested_fps = document.number("src_fps", -1.);
    let media_enabled = document.boolean("save_raw") || document.boolean("save_debug");
    if media_enabled || loaded.config.spherical_map.is_some() {
        source.enable_color();
    }
    let fps = if requested_fps > 0. {
        requested_fps
    } else if media_enabled {
        source.recording_fps()?
    } else {
        1.0
    };
    // The inactive recorder uses no frame rate; encoded output always uses metadata or observed PTS.
    let map_settings = loaded.config.spherical_map.clone();
    let mut tracker = Tracker::new(loaded.config.clone())?;
    if let Some(template) = document
        .get("sphere_map_fn")
        .filter(|value| !value.is_empty())
    {
        let mut path = PathBuf::from(template.trim_matches('"'));
        if !path.is_absolute() && !path.exists() {
            path = config_path.parent().unwrap_or(Path::new(".")).join(path);
        }
        tracker.load_sphere_template(fictrac_io::load_sphere_map(&path)?)?;
    }
    let mut sinks = fictrac_io::OutputSinks::from_config(&document)?;
    let mut source = source.prefetch()?;
    // Validate the first decoded frame before creating output artifacts.
    let first = next_tracking(&mut source, map_settings.as_ref())?
        .ok_or("input contains no video frames")?;
    create_run_dir(output)?;
    let mut dat = new_file(output.join("tracking.dat.partial"))?;
    let mut all_frames = new_file(output.join("all-frames.dat.partial"))?;
    let mut quality = new_file(output.join("quality.csv.partial"))?;
    let mut media =
        fictrac_control::MediaRecorders::create(output, &document, &loaded.config, fps)?;
    let mut diagnostics = if media.active() {
        Some(fictrac_core::Diagnostics::new(
            loaded.config.clone(),
            document.number(
                "q_factor",
                fictrac_core::MapSettings::default().quality as f64,
            ) as usize,
            document.number("thr_ratio", 1.25),
            document.number("thr_win_pc", 0.25),
        )?)
    } else {
        None
    };
    writeln!(
        quality,
        "frame,timestamp_ms,accepted,status,fit_error,samples"
    )?;
    let spherical = loaded.config.spherical_map.is_some();
    let mut summary = Summary {
        version: env!("CARGO_PKG_VERSION"),
        input: fs::canonicalize(input)?.to_string_lossy().into_owned(),
        video: source.info.clone(),
        effective_config: loaded.config,
        frames: 0,
        tracked: 0,
        relocalized: 0,
        rejected: 0,
        initializing: 0,
        mean_error: 0.0,
        max_error: 0.0,
        wall_seconds: 0.0,
        end_to_end_fps: 0.0,
        tracking_fps: 0.0,
        input_duration_ms: 0.0,
        final_position: [0.0; 2],
        final_heading: 0.0,
        estimator: if spherical {
            "accumulated spherical map / pure Rust BOBYQA"
        } else {
            "perspective pairwise robust photometric SO(3)"
        },
        error_metric: if spherical {
            "mean squared binary intensity (0..65025)"
        } else {
            "normalized luma RMSE (0..1)"
        },
        warnings: loaded.warnings,
    };
    let mut track_seconds = 0.0;
    let mut points = vec![];
    let mut current = Some(first);
    while let Some(frame) = current {
        let frame = if requested_fps > 0. {
            frame.with_timestamp(summary.frames as f64 * 1000. / requested_fps)?
        } else {
            frame
        };
        let debug_frame = media.active().then(|| frame.clone());
        let start = Instant::now();
        let row = tracker.process(frame)?;
        track_seconds += start.elapsed().as_secs_f64();
        if row.accepted || !summary.effective_config.legacy_roi_coordinates {
            write_dat(&mut dat, &row)?;
        }
        write_dat(&mut all_frames, &row)?;
        for warning in sinks.write(&row) {
            if summary.warnings.len() < 100 && !summary.warnings.contains(&warning) {
                eprintln!("warning: {warning}");
                summary.warnings.push(warning);
            }
        }
        writeln!(
            quality,
            "{},{:.6},{},{},{:.9},{}",
            row.frame, row.timestamp_ms, row.accepted, row.status, row.error, row.samples
        )?;
        summary.frames += 1;
        match row.status {
            "tracked" => summary.tracked += 1,
            "relocalized" => summary.relocalized += 1,
            "rejected" | "recovering" => summary.rejected += 1,
            "initializing" => summary.initializing += 1,
            _ => return Err("tracker returned an unknown quality status".into()),
        }
        summary.mean_error += row.error;
        summary.max_error = summary.max_error.max(row.error);
        summary.final_position = row.position;
        summary.final_heading = row.heading;
        summary.input_duration_ms = row.timestamp_ms;
        // Bound preview memory independently of recording length (data remains streamed).
        if points.len() >= 100_000 {
            points = points.iter().step_by(2).copied().collect();
        }
        points.push(row.position);
        if let Some(frame) = debug_frame {
            let rgb = source
                .take_rgb()
                .unwrap_or_else(|| fictrac_control::gray_rgb(&frame));
            let views = tracker
                .debug_views()
                .cloned()
                .or_else(|| diagnostics.as_mut().map(|d| d.update(&frame, Some(&row))))
                .ok_or("diagnostic images unavailable for recording")?;
            media.write_row(&rgb, &frame, &views, &points, &row)?;
        }
        if summary.frames.is_multiple_of(100) {
            eprintln!("{} frames · {} rejected", summary.frames, summary.rejected);
        }
        if max_frames.is_some_and(|n| summary.frames >= n) {
            break;
        }
        current = next_tracking(&mut source, map_settings.as_ref())?;
    }
    if summary.frames < 2 {
        return Err("need at least two frames to estimate motion; partial output retained".into());
    }
    media.finish()?;
    if let Some(template) = tracker.sphere_template() {
        fictrac_io::save_sphere_map(&output.join("sphere-map.png"), &template)?;
    }
    all_frames.flush()?;
    all_frames.get_ref().sync_all()?;
    drop(all_frames);
    dat.flush()?;
    quality.flush()?;
    dat.get_ref().sync_all()?;
    quality.get_ref().sync_all()?;
    drop(dat);
    drop(quality);
    let mut svg = new_file(output.join("path.svg.partial"))?;
    write_path_svg(&mut svg, &points)?;
    svg.flush()?;
    drop(svg);
    summary.wall_seconds = wall.elapsed().as_secs_f64();
    summary.end_to_end_fps = summary.frames as f64 / summary.wall_seconds.max(1e-9);
    summary.tracking_fps = summary.frames as f64 / track_seconds.max(1e-9);
    summary.mean_error /= summary.frames as f64;
    let mut json = new_file(output.join("summary.json.partial"))?;
    serde_json::to_writer_pretty(&mut json, &summary)?;
    writeln!(json)?;
    json.flush()?;
    drop(json);
    // Promote only after the complete run succeeds. Interrupted runs keep .partial files.
    for name in [
        "tracking.dat",
        "all-frames.dat",
        "quality.csv",
        "path.svg",
        "summary.json",
    ] {
        fs::rename(output.join(format!("{name}.partial")), output.join(name))?;
    }
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(summary)
}
fn run() -> Result<()> {
    match parse_cli()?.command {
        Command::Schema => println!(
            "{}",
            serde_json::to_string_pretty(&fictrac_control::schema())?
        ),
        Command::Agent => {
            fictrac_control::serve(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())?
        }
        Command::Mcp => {
            fictrac_control::serve_mcp(&mut std::io::stdin().lock(), &mut std::io::stdout().lock())?
        }
        Command::Config {
            config,
            fields,
            output,
        } => {
            if fs::metadata(&config)?.len() > 1_000_000 {
                return Err("config exceeds 1 MB".into());
            }
            let mut document = fictrac_io::ConfigDocument::parse(fs::read_to_string(config)?)?;
            for field in fields {
                let (key, value) = field.split_once('=').ok_or("--set requires key=value")?;
                document.set(key, value)?;
            }
            if let Some(output) = output {
                let mut file = new_file(output)?;
                file.write_all(document.text().as_bytes())?;
                file.flush()?;
            } else {
                print!("{}", document.text());
            }
        }
        Command::Compare {
            reference,
            candidate,
            report,
            max_path_rmse,
            max_heading_rmse,
            max_rotation_rmse,
        } => {
            if [max_path_rmse, max_heading_rmse, max_rotation_rmse]
                .iter()
                .any(|v| !v.is_finite() || *v <= 0.0)
            {
                return Err("comparison limits must be finite and positive".into());
            }
            let comparison = fictrac_io::compare_dat(
                std::io::BufReader::new(File::open(reference)?),
                std::io::BufReader::new(File::open(candidate)?),
            )?;
            let passed = comparison.passes(max_path_rmse, max_heading_rmse, max_rotation_rmse);
            let value = serde_json::json!({"comparison":comparison,"passed":passed,"limits":{"path_rmse_sphere_rad":max_path_rmse,"heading_rmse_rad":max_heading_rmse,"rotation_component_rmse_rad":max_rotation_rmse}});
            println!("{}", serde_json::to_string_pretty(&value)?);
            if let Some(path) = report {
                let mut out = new_file(path)?;
                serde_json::to_writer_pretty(&mut out, &value)?;
                out.flush()?;
            }
            if !passed {
                return Err("reference comparison failed its declared tolerances".into());
            }
        }
        Command::Track {
            config,
            input,
            output,
            max_frames,
        } => {
            let input = match input {
                Some(p) => p,
                None => {
                    let hint =
                        load_source_hint(&config)?.ok_or("provide --input or a config source")?;
                    let path = PathBuf::from(hint);
                    if path.is_absolute() || path.exists() {
                        path
                    } else {
                        config.parent().unwrap_or(Path::new(".")).join(path)
                    }
                }
            };
            track(&input, &config, &output, max_frames)?;
        }
        Command::Inspect {
            input,
            decode,
            preview,
        } => {
            let mut source = VideoSource::open(&input)?;
            println!("{}", serde_json::to_string_pretty(&source.info)?);
            if decode || preview.is_some() {
                let mut count = 0;
                let mut last = 0.0;
                while let Some(frame) = source.next_frame()? {
                    if count == 0
                        && let Some(path) = &preview
                    {
                        let mut out = new_file(path)?;
                        writeln!(out, "P5\n{} {}\n255", frame.width(), frame.height())?;
                        out.write_all(
                            &frame
                                .pixels()
                                .iter()
                                .map(|p| (p * 255.0).round() as u8)
                                .collect::<Vec<_>>(),
                        )?;
                        out.flush()?;
                    }
                    count += 1;
                    last = frame.timestamp_ms();
                    if !decode {
                        break;
                    }
                }
                println!("decoded_frames={count} last_timestamp_ms={last:.6}");
            }
        }
        Command::Calibrate {
            width,
            height,
            vfov,
            points,
            lab_rotation,
            output,
        } => {
            let points: Vec<[f64; 2]> = serde_json::from_str(&points)?;
            let sphere = Geometry::fit_sphere(width, height, vfov, &points)?;
            let config = Config {
                width,
                height,
                vfov_deg: vfov,
                sphere,
                camera_to_lab: serde_json::from_str(&lab_rotation)?,
                ..Config::default()
            };
            config.validate()?;
            let mut out = new_file(output)?;
            serde_json::to_writer_pretty(&mut out, &config)?;
            writeln!(out)?;
            out.flush()?;
        }
        Command::Demo { output, frames } => {
            if !(3..=10000).contains(&frames) {
                return Err("demo frames must be 3..10000".into());
            }
            create_run_dir(&output)?;
            let config = Config {
                source: Some("sphere.y4m".into()),
                ..Config::default()
            };
            let mut json = new_file(output.join("config.json"))?;
            serde_json::to_writer_pretty(&mut json, &config)?;
            json.flush()?;
            let mut video = new_file(output.join("sphere.y4m"))?;
            write_y4m_header(&mut video, config.width, config.height, 60)?;
            let delta = [0.006, -0.009, 0.004];
            for i in 0..frames {
                let frame = synthetic::render(
                    &config,
                    synthetic::orientation_at(delta, i),
                    i as f64 * 1000.0 / 60.0,
                )?;
                write_y4m_frame(&mut video, &frame)?;
            }
            video.flush()?;
            drop(video);
            let summary = track(
                &output.join("sphere.y4m"),
                &output.join("config.json"),
                &output.join("tracking"),
                None,
            )?;
            let text = fs::read_to_string(output.join("tracking/tracking.dat"))?;
            if text.lines().count() != frames {
                return Err("synthetic data has an unexpected frame count".into());
            }
            let mut squared = 0.0;
            for line in text.lines().skip(1) {
                let values: Vec<f64> = line
                    .split(',')
                    .take(4)
                    .map(|s| s.trim().parse())
                    .collect::<std::result::Result<_, _>>()?;
                let values: [f64; 4] = values
                    .try_into()
                    .map_err(|_| "synthetic data row has fewer than four columns")?;
                for i in 0..3 {
                    squared += (values[i + 1] - delta[i]).powi(2);
                }
            }
            let rmse = (squared / (3 * (frames - 1)) as f64).sqrt();
            let oracle = serde_json::json!({"expected_delta":delta,"rotation_component_rmse_rad":rmse,"tolerance_rad":0.003,"passed":rmse<0.003&&summary.rejected==0});
            let mut out = new_file(output.join("oracle.json"))?;
            serde_json::to_writer_pretty(&mut out, &oracle)?;
            out.flush()?;
            println!("{}", serde_json::to_string_pretty(&oracle)?);
            if !rmse.is_finite() || rmse >= 0.003 || summary.rejected > 0 {
                return Err("synthetic accuracy gate failed".into());
            }
        }
    }
    Ok(())
}
/// Upstream accepts `fictrac config.txt`; retain that useful invocation for video sources.
fn parse_cli() -> Result<Cli> {
    let args: Vec<_> = std::env::args_os().collect();
    if let Some(config) = args.get(1)
        && Path::new(config).is_file()
    {
        let mut translated = vec![
            args[0].clone(),
            "track".into(),
            "--config".into(),
            config.clone(),
        ];
        translated.extend(args.iter().skip(2).cloned());
        if !args.iter().skip(2).any(|value| {
            let text = value.to_string_lossy();
            text == "--output"
                || text.starts_with("--output=")
                || (text.starts_with("-o") && !text.starts_with("--"))
        }) {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let path = Path::new(config);
            if fs::metadata(path)?.len() > 1_000_000 {
                return Err("config exceeds 1 MB".into());
            }
            let text = fs::read_to_string(path)?;
            let document = if text.trim_start().starts_with('{') {
                None
            } else {
                Some(fictrac_io::ConfigDocument::parse(text)?)
            };
            let explicit_input = args.iter().skip(2).collect::<Vec<_>>();
            let input = explicit_input
                .windows(2)
                .find(|pair| pair[0] == "--input" || pair[0] == "-i")
                .map(|pair| PathBuf::from(pair[1]))
                .or_else(|| {
                    explicit_input.iter().find_map(|value| {
                        value
                            .to_str()
                            .and_then(|text| text.strip_prefix("--input="))
                            .map(PathBuf::from)
                    })
                })
                .or_else(|| {
                    load_source_hint(path)
                        .ok()
                        .flatten()
                        .filter(|source| source.parse::<u32>().is_err())
                        .map(PathBuf::from)
                });
            let prefix = document
                .as_ref()
                .and_then(|doc| doc.get("output_fn"))
                .map(|value| value.trim_matches('"'))
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    input
                        .as_ref()
                        .and_then(|path| path.file_stem())
                        .filter(|stem| !stem.is_empty())
                        .map(PathBuf::from)
                })
                .unwrap_or_else(|| PathBuf::from("fictrac"));
            let mut output = prefix.into_os_string();
            output.push(format!("-{stamp}"));
            translated.push("--output".into());
            translated.push(output);
        }
        return Ok(Cli::parse_from(translated));
    }
    Ok(Cli::parse_from(args))
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("hejjetrac-cli: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn next_tracking(
    source: &mut fictrac_io::PrefetchedVideoSource,
    settings: Option<&fictrac_core::MapSettings>,
) -> Result<Option<fictrac_core::GrayFrame>> {
    Ok(if let Some(s) = settings {
        source.next_tracking_frame(s.channel)?
    } else {
        source.next_frame()?
    })
}
