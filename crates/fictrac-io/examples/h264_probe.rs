//! Development-only serial H264 benchmark using real captured camera stills.
//!
//! The saved measurements use `runs/camera-validation-v03/paused-snapshot/input.png`
//! and `runs/camera-validation-v03/stepped-snapshot/input.png`, native 1920x1080
//! RGB camera captures. Each case encodes paused/stepped/paused, serially.
//! The timer includes RGB->YUV420, send_frame and packet draining; it excludes
//! PNG read/resize/edge padding, encoder construction and disk writes.
//! `default` reproduces the former Baseline auto profile at QP20/GOP30. `high`
//! selects High100/no Intra4x4/GOP30. `high_intra` selects GOP1, and
//! `high_intra_nodeblock` additionally disables deblocking. This development
//! probe does not select the application's current production defaults.
use oxideav_core::{
    CodecId, CodecParameters, Encoder, Frame, PixelFormat, Rational, VideoFrame, VideoPlane,
};
use oxideav_h264::h264_encoder::H264CodecEncoder;
use std::{fs, io::Write, path::PathBuf, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 7 {
        return Err("usage: h264_probe INPUT1 INPUT2 WIDTH HEIGHT MODE OUTPUT_DIR; modes default,high,high_intra,high_intra_nodeblock".into());
    }
    let width: usize = args[3].parse()?;
    let height: usize = args[4].parse()?;
    if !(16..=8192).contains(&width)
        || !(16..=8192).contains(&height)
        || width.checked_mul(height).is_none_or(|n| n > 32_000_000)
    {
        return Err("dimensions must be 16..8192, at most 32M pixels".into());
    }
    if !matches!(
        args[5].as_str(),
        "default" | "high" | "high_intra" | "high_intra_nodeblock"
    ) {
        return Err(
            "unknown MODE; choose default, high, high_intra, or high_intra_nodeblock".into(),
        );
    }
    let mode = &args[5];
    let dir = PathBuf::from(&args[6]);
    fs::create_dir_all(&dir)?;
    let coded_width = width.div_ceil(16) * 16;
    let coded_height = height.div_ceil(16) * 16;
    let mut prepared = vec![];
    for name in [&args[1], &args[2]] {
        let image = image::open(name)?
            .resize_exact(
                width as u32,
                height as u32,
                image::imageops::FilterType::Triangle,
            )
            .into_rgb8();
        let mut rgb = vec![0; coded_width * coded_height * 3];
        for y in 0..coded_height {
            for x in 0..coded_width {
                let src = ((y.min(height - 1)) * width + x.min(width - 1)) * 3;
                let dst = (y * coded_width + x) * 3;
                rgb[dst..dst + 3].copy_from_slice(&image.as_raw()[src..src + 3]);
            }
        }
        prepared.push(rgb);
    }
    let mut params = CodecParameters::video(CodecId::new("h264"));
    params.width = Some(coded_width as u32);
    params.height = Some(coded_height as u32);
    params.pixel_format = Some(PixelFormat::Yuv420P);
    params.frame_rate = Some(Rational::new(30, 1));
    params.options.insert("qp", "20");
    params.options.insert("format", "annexb");
    params
        .options
        .insert("gop", if mode.contains("intra") { "1" } else { "30" });
    if mode != "default" {
        params.options.insert("profile", "high");
        params.options.insert("intra_4x4", "false");
        if mode.contains("nodeblock") {
            params.options.insert("deblock", "false");
        }
    }
    let begin = Instant::now();
    let mut encoder = H264CodecEncoder::new(&params)?;
    let init_ms = begin.elapsed().as_secs_f64() * 1000.;
    let mut encoded = Vec::new();
    let mut frame_ms = vec![];
    let mut packet_sizes = vec![];
    let mut yuv_reference = Vec::new();
    for index in 0..3 {
        let start = Instant::now();
        let frame = Frame::Video(yuv(
            &prepared[index % 2],
            coded_width,
            coded_height,
            index as i64,
        ));
        encoder.send_frame(&frame)?;
        loop {
            match encoder.receive_packet() {
                Ok(packet) => {
                    packet_sizes.push(packet.data.len());
                    encoded.extend(packet.data);
                }
                Err(oxideav_core::Error::NeedMore | oxideav_core::Error::Eof) => break,
                Err(e) => return Err(e.into()),
            }
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        frame_ms.push(elapsed);
        if let Frame::Video(frame) = &frame {
            for p in &frame.planes {
                yuv_reference.extend_from_slice(&p.data);
            }
        }
        println!("frame {index}: {elapsed:.3} ms");
        std::io::stdout().flush()?;
    }
    encoder.flush()?;
    loop {
        match encoder.receive_packet() {
            Ok(packet) => encoded.extend(packet.data),
            Err(oxideav_core::Error::NeedMore | oxideav_core::Error::Eof) => break,
            Err(e) => return Err(e.into()),
        }
    }
    fs::write(dir.join("encoded.h264"), &encoded)?;
    fs::write(dir.join("reference.yuv"), &yuv_reference)?;
    fs::write(dir.join("input.rgb"), &prepared[0])?;
    let total_ms: f64 = frame_ms.iter().sum();
    let report = serde_json::json!({"width":width,"height":height,"coded_width":coded_width,"coded_height":coded_height,"mode":mode,"frames":3,"inputs":[&args[1],&args[2]],"init_ms":init_ms,"per_frame_ms":frame_ms,"total_encode_ms":total_ms,"encoding_fps":3000./total_ms,"packet_sizes":packet_sizes,"encoded_bytes":encoded.len(),"boundary":"RGB->YUV420 + send_frame + packet drain; excludes PNG read/resize,encoder init,disk writes. Serial three frames paused/stepped/paused."});
    fs::write(dir.join("timing.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string(&report)?);
    Ok(())
}
fn yuv(rgb: &[u8], w: usize, h: usize, pts: i64) -> VideoFrame {
    let mut y = vec![0; w * h];
    let mut u = vec![0; w * h / 4];
    let mut v = vec![0; w * h / 4];
    for row in 0..h {
        for col in 0..w {
            let i = (row * w + col) * 3;
            let r = rgb[i] as i32;
            let g = rgb[i + 1] as i32;
            let b = rgb[i + 2] as i32;
            y[row * w + col] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
        }
    }
    for row in (0..h).step_by(2) {
        for col in (0..w).step_by(2) {
            let mut r = 0;
            let mut g = 0;
            let mut b = 0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = ((row + dy) * w + col + dx) * 3;
                    r += rgb[i] as i32;
                    g += rgb[i + 1] as i32;
                    b += rgb[i + 2] as i32;
                }
            }
            r = (r + 2) / 4;
            g = (g + 2) / 4;
            b = (b + 2) / 4;
            let i = (row / 2) * (w / 2) + col / 2;
            u[i] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            v[i] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
    VideoFrame {
        pts: Some(pts),
        planes: vec![
            VideoPlane { stride: w, data: y },
            VideoPlane {
                stride: w / 2,
                data: u,
            },
            VideoPlane {
                stride: w / 2,
                data: v,
            },
        ],
    }
}
