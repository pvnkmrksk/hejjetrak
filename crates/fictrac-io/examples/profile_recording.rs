//! Development-only serial phase profile of calibrated debug rendering and codecs.
//! Uses saved native source/diagnostic PNGs; no camera, tracking, or subprocess.
use fictrac_core::DebugViews;
use fictrac_io::{
    CalibratedDebugRenderer, RecordingCodec, VideoRecorder, load_config,
    render_calibrated_debug_rgb,
};
use std::{fs, path::Path, time::Instant};

fn gray(path: &Path) -> Result<image::GrayImage, Box<dyn std::error::Error>> {
    Ok(image::open(path)?.into_luma8())
}

fn stats(values: &[f64]) -> serde_json::Value {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let sum: f64 = values.iter().sum();
    serde_json::json!({"per_frame_ms":values,"mean_ms":sum/values.len() as f64,
        "median_ms":sorted[sorted.len()/2],"p95_ms":sorted[(sorted.len()*95/100).min(sorted.len()-1)],
        "frames_per_second":1000.0*values.len() as f64/sum})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "usage: profile_recording SNAPSHOT_DIRECTORY CONFIG OUTPUT_DIRECTORY ITERATIONS".into(),
        );
    }
    let count: usize = args[4].parse()?;
    if !(5..=1000).contains(&count) {
        return Err("iterations must be 5..1000".into());
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[3]);
    fs::create_dir(output)?;
    let source = image::open(input.join("input.png"))?.into_rgb8();
    let config = load_config(
        Path::new(&args[2]),
        source.width() as usize,
        source.height() as usize,
    )?
    .config;
    let roi = gray(&input.join("roi.png"))?;
    let diff = gray(&input.join("warped-diff.png"))?;
    let instant = gray(&input.join("instant-map.png"))?;
    let accumulated = gray(&input.join("accumulated-map.png"))?;
    if roi.width() != roi.height()
        || diff.dimensions() != roi.dimensions()
        || accumulated.dimensions() != instant.dimensions()
    {
        return Err("snapshot diagnostic PNG dimensions do not agree".into());
    }
    let views = DebugViews {
        roi_size: roi.width() as usize,
        map_width: instant.width() as usize,
        map_height: instant.height() as usize,
        roi: roi.into_raw(),
        diff: diff.into_raw(),
        instant: instant.into_raw(),
        accumulated: accumulated.into_raw(),
    };
    let path = [[0.0, 0.0], [0.1, 0.2], [0.2, 0.3]];
    let poses = [[0.0, 0.0, 0.0], [0.01, 0.02, 0.03], [0.02, 0.03, 0.04]];
    let mut debug =
        render_calibrated_debug_rgb(source.as_raw(), &config, &views, &path, &poses, 160)?;
    let mut render_ms = vec![];
    for _ in 0..count {
        let begin = Instant::now();
        debug = std::hint::black_box(render_calibrated_debug_rgb(
            source.as_raw(),
            &config,
            &views,
            &path,
            &poses,
            160,
        )?);
        render_ms.push(begin.elapsed().as_secs_f64() * 1000.0);
    }
    let begin = Instant::now();
    let renderer = CalibratedDebugRenderer::new(&config, 160)?;
    let cache_init_ms = begin.elapsed().as_secs_f64() * 1000.0;
    let mut cached_render_ms = vec![];
    let mut cached = vec![];
    for _ in 0..count {
        let begin = Instant::now();
        cached = std::hint::black_box(renderer.render(source.as_raw(), &views, &path, &poses)?);
        cached_render_ms.push(begin.elapsed().as_secs_f64() * 1000.0);
    }
    if cached != debug {
        return Err(
            "cached render bytes differ from independent direct-projection renderer".into(),
        );
    }
    image::save_buffer(
        output.join("rendered.png"),
        &debug,
        640,
        480,
        image::ColorType::Rgb8,
    )?;
    let mut cases = vec![];
    for (name, codec) in [
        ("h264", RecordingCodec::H264),
        ("mjpg", RecordingCodec::Mjpeg),
        ("raw", RecordingCodec::Raw),
    ] {
        for (view, width, height, rgb) in [
            (
                "raw",
                source.width() as usize,
                source.height() as usize,
                source.as_raw().as_slice(),
            ),
            ("debug", 640, 480, debug.as_slice()),
        ] {
            let begin = Instant::now();
            let mut recorder = VideoRecorder::create(
                &output.join(format!("{name}-{view}.avi")),
                width,
                height,
                100.0,
                codec,
            )?;
            let init_ms = begin.elapsed().as_secs_f64() * 1000.0;
            let mut write_ms = vec![];
            for _ in 0..count {
                let begin = Instant::now();
                recorder.write_rgb(std::hint::black_box(rgb))?;
                write_ms.push(begin.elapsed().as_secs_f64() * 1000.0);
            }
            let begin = Instant::now();
            recorder.finish()?;
            let finish_ms = begin.elapsed().as_secs_f64() * 1000.0;
            cases.push(serde_json::json!({"codec":name,"view":view,"width":width,"height":height,"frames":count,"init_ms":init_ms,"write":stats(&write_ms),"finish_ms":finish_ms}));
        }
    }
    let report = serde_json::json!({"snapshot":args[1],"config":args[2],"iterations":count,
        "source_dimensions":[source.width(),source.height()],"debug_dimensions":[640,480],"path_points":path.len(),"poses":poses.len(),
        "render":stats(&render_ms),"cached_render":stats(&cached_render_ms),"cache_init_ms":cache_init_ms,
        "cached_and_direct_pixels_exact":true,"codecs":cases,
        "boundary":"Serial saved native PNGs, unchanged frame repeated; PNG read/decode and initial warmup render excluded. Direct render includes validation, geometry construction, remap, overlays, allocation. Cached render excludes one-time validated geometry construction but includes dynamic views/overlays, RGB sampling, and allocation. Codec write includes RGB conversion, encode, packet drain and AVI write; init/finalization separately timed. No capture, tracking, UI, or concurrency; not sustained camera FPS."});
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
