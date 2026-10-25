//! Predecode once, then measure only the shared numerical engine per frame.
use fictrac_core::Tracker;
use fictrac_io::{VideoSource, compare_dat, load_config, write_dat};
use std::{fs, hint::black_box, io::BufReader, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: profile_tracking VIDEO CONFIG REPEATS OUTPUT_DIRECTORY".into());
    }
    let repeats: usize = args[2].parse()?;
    if !(2..=100).contains(&repeats) {
        return Err("repeats must be 2..100 (plus one untimed warmup)".into());
    }
    let output = PathBuf::from(&args[3]);
    fs::create_dir(&output)?;
    let mut source = VideoSource::open(args[0].as_ref())?;
    let config = load_config(args[1].as_ref(), source.info.width, source.info.height)?.config;
    let mut frames = Vec::new();
    let mut bytes = 0usize;
    while let Some(frame) = if let Some(map) = &config.spherical_map {
        source.next_tracking_frame(map.channel)?
    } else {
        source.next_frame()?
    } {
        bytes = bytes
            .checked_add(frame.pixels().len() * 4)
            .ok_or("input size overflow")?;
        if bytes > 512 * 1024 * 1024 {
            return Err("profiling fixture exceeds 512 MiB decoded-frame bound".into());
        }
        frames.push(frame);
    }
    drop(source);
    if frames.len() < 2 {
        return Err("need at least two frames".into());
    }
    let mut runs = Vec::new();
    let mut final_rows = Vec::new();
    for iteration in 0..=repeats {
        let mut tracker = Tracker::new(config.clone())?;
        let mut times = Vec::new();
        let mut rows = Vec::new();
        for frame in &frames {
            let tick = Instant::now();
            let row = black_box(tracker.process(black_box(frame.clone()))?);
            let micros = tick.elapsed().as_secs_f64() * 1e6;
            if row.frame > 0 {
                times.push(micros);
            }
            rows.push(row);
        }
        if iteration > 0 {
            let mean = times.iter().sum::<f64>() / times.len() as f64;
            times.sort_by(f64::total_cmp);
            runs.push(serde_json::json!({"mean_us":mean,"median_us":times[times.len()/2],
                "p95_us":times[(times.len()*95/100).min(times.len()-1)],"max_us":times[times.len()-1],
                "frames":rows.len(),"rejected":rows.iter().filter(|r|!r.accepted).count()}));
        }
        final_rows = rows;
    }
    let mut dat = Vec::new();
    for row in &final_rows {
        write_dat(&mut dat, row)?;
    }
    fs::write(output.join("all-frames.dat"), &dat)?;
    let reference =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/benchmark/upstream.dat");
    let comparison = compare_dat(
        BufReader::new(fs::File::open(reference)?),
        BufReader::new(dat.as_slice()),
    )?;
    let passed = comparison.passes(0.075, 0.025, 0.020);
    let mut means: Vec<_> = runs.iter().filter_map(|r| r["mean_us"].as_f64()).collect();
    means.sort_by(f64::total_cmp);
    let report = serde_json::json!({"input":args[0],"config":config,"frames":frames.len(),"repeats":repeats,
        "boundary":"Tracker::process only, including ROI, fitting, integration, map and numerical debug views; predecoded/preconverted immutable frames; no I/O, capture, pacing, UI or recording; initialization excluded",
        "median_run_mean_us":means[means.len()/2],"runs":runs,"reference_comparison":comparison,"passed":passed});
    fs::write(
        output.join("report.json"),
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !passed {
        return Err("trajectory/heading/rotation reference gates failed; see report.json".into());
    }
    Ok(())
}
