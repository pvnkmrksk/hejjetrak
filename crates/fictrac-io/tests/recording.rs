use fictrac_core::{DebugViews, SphereTemplate, TrackRow};
use fictrac_io::{
    ConfigDocument, OutputSinks, RecordingCodec, VideoRecorder, VideoSource, load_sphere_map,
    render_debug_rgb, save_sphere_map,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
fn directory(label: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path =
        std::env::temp_dir().join(format!("fictrac-io-{label}-{}-{nonce}", std::process::id()));
    fs::create_dir(&path)?;
    Ok(path)
}
fn row() -> TrackRow {
    TrackRow {
        frame: 4,
        sequence: 7,
        delta_camera: [0.01, 0.02, 0.03],
        error: 10.,
        delta_lab: [0.01, 0.02, 0.03],
        absolute_camera: [0.; 3],
        absolute_lab: [0.; 3],
        position: [0.1, 0.2],
        heading: 0.3,
        direction: 0.4,
        speed: 0.5,
        integrated: [0.1, 0.2],
        timestamp_ms: 100.,
        delta_ms: 33.33,
        accepted: true,
        status: "tracked",
        samples: 100,
    }
}
#[test]
fn png_template_preserves_all_confidence_values_and_rejects_overwrite()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("template")?;
    let file = dir.join("sphere.png");
    let pixels = (0..1800).map(|i| (i % 256) as u8).collect();
    let template = SphereTemplate::new(60, 30, pixels)?;
    save_sphere_map(&file, &template)?;
    let roundtrip = load_sphere_map(&file)?;
    assert_eq!(roundtrip.pixels, template.pixels);
    assert_eq!((roundtrip.width, roundtrip.height), (60, 30));
    assert!(save_sphere_map(&file, &template).is_err());
    assert!(save_sphere_map(&dir.join("lossy.jpg"), &template).is_err());
    fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn h264_recording_reopens_with_original_timing_and_color() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = directory("h264")?;
    let path = dir.join("color.mp4");
    let mut recorder = VideoRecorder::create(&path, 32, 32, 30., RecordingCodec::H264Mp4)?;
    for frame in 0..4 {
        let mut rgb = vec![0u8; 32 * 32 * 3];
        for y in 0..32 {
            for x in 0..32 {
                let i = (y * 32 + x) * 3;
                rgb[i] = (x * 4 + frame * 8) as u8;
                rgb[i + 1] = (y * 4) as u8;
                rgb[i + 2] = 64;
            }
        }
        recorder.write_rgb(&rgb)?;
    }
    assert_eq!(recorder.frame_count(), 4);
    assert!(recorder.write_rgb(&[0; 3]).is_err());
    recorder.finish()?;
    let mut source = VideoSource::open(&path)?;
    source.enable_color();
    assert_eq!((source.info.width, source.info.height), (32, 32));
    let mut count = 0;
    while let Some(frame) = source.next_frame()? {
        assert!((frame.timestamp_ms() - count as f64 * 1000. / 30.).abs() < 0.001);
        let rgb = source.take_rgb().ok_or("missing RGB")?;
        let center = (16 * 32 + 16) * 3;
        assert!((rgb[center] as i32 - (64 + count * 8)).abs() < 16);
        assert!((rgb[center + 1] as i32 - 64).abs() < 16);
        assert!((rgb[center + 2] as i32 - 64).abs() < 16);
        count += 1;
    }
    assert_eq!(count, 4);
    assert!(VideoRecorder::create(&path, 32, 32, 30., RecordingCodec::H264Mp4).is_err());
    fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn avi_mjpeg_has_real_jpeg_frames_and_complete_index() -> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("mjpeg")?;
    let path = dir.join("debug.avi");
    let mut recorder = VideoRecorder::create(&path, 32, 16, 25., RecordingCodec::Mjpeg)?;
    let rgb = [220, 60, 20].repeat(32 * 16);
    recorder.write_rgb(&rgb)?;
    recorder.write_rgb(&rgb)?;
    recorder.finish()?;
    let mut source = VideoSource::open(&path)?;
    source.enable_color();
    assert!(source.next_frame()?.is_some());
    assert!(source.next_frame()?.is_some());
    assert!(source.next_frame()?.is_none());
    let bytes = fs::read(&path)?;
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into()?) as usize + 8,
        bytes.len()
    );
    let movi = bytes
        .windows(4)
        .position(|v| v == b"movi")
        .ok_or("no movi list")?;
    let start = movi + 4;
    assert_eq!(&bytes[start..start + 4], b"00dc");
    let size = u32::from_le_bytes(bytes[start + 4..start + 8].try_into()?) as usize;
    let image = image::load_from_memory(&bytes[start + 8..start + 8 + size])?.to_rgb8();
    assert_eq!(image.dimensions(), (32, 16));
    let pixel = image.get_pixel(8, 8);
    assert!((pixel[0] as i32 - 220).abs() < 5);
    assert!((pixel[1] as i32 - 60).abs() < 5);
    let index = bytes
        .windows(4)
        .rposition(|v| v == b"idx1")
        .ok_or("no AVI index")?;
    assert_eq!(
        u32::from_le_bytes(bytes[index + 4..index + 8].try_into()?),
        32
    );
    fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn avi_raw_preserves_bgr_rows_and_padding() -> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("raw")?;
    let path = dir.join("raw.avi");
    let mut recorder = VideoRecorder::create(&path, 17, 16, 60., RecordingCodec::Raw)?;
    let mut rgb = vec![0; 17 * 16 * 3];
    rgb[..3].copy_from_slice(&[1, 2, 3]);
    let end = rgb.len();
    rgb[end - 3..].copy_from_slice(&[10, 20, 30]);
    recorder.write_rgb(&rgb)?;
    recorder.finish()?;
    let mut source = VideoSource::open(&path)?;
    source.enable_color();
    assert!(source.next_frame()?.is_some());
    assert_eq!(source.take_rgb().ok_or("missing raw RGB")?, rgb);
    let bytes = fs::read(&path)?;
    let movi = bytes
        .windows(4)
        .position(|v| v == b"movi")
        .ok_or("no movi list")?;
    let offset = movi + 12;
    assert_eq!(&bytes[offset - 8..offset - 4], b"00db");
    assert_eq!(&bytes[offset + 48..offset + 51], &[30, 20, 10]);
    assert_eq!(&bytes[offset + 15 * 52..offset + 15 * 52 + 3], &[3, 2, 1]);
    assert_eq!(bytes[offset + 51], 0);
    fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn udp_and_dat_share_full_row_with_actual_sequence_and_omit_rejection()
-> Result<(), Box<dyn std::error::Error>> {
    let receiver = std::net::UdpSocket::bind("127.0.0.1:0")?;
    receiver.set_read_timeout(Some(Duration::from_millis(150)))?;
    let config = ConfigDocument::parse(format!(
        "sock_host : 127.0.0.1\nsock_port : {}\nzmq_port : 0\n",
        receiver.local_addr()?.port()
    ))?;
    let mut sinks = OutputSinks::from_config(&config)?;
    assert!(sinks.active());
    assert!(sinks.write(&row()).is_empty());
    let mut data = [0; 4096];
    let n = receiver.recv(&mut data)?;
    let line = std::str::from_utf8(&data[..n])?;
    let values: Vec<_> = line.trim().split(',').map(str::trim).collect();
    assert_eq!(values.len(), 26);
    assert_eq!(values[0], "FT");
    assert_eq!(values[1], "4");
    assert_eq!(values[23], "7");
    let mut rejected = row();
    rejected.accepted = false;
    assert!(sinks.write(&rejected).is_empty());
    assert!(receiver.recv(&mut data).is_err());
    Ok(())
}
#[cfg(unix)]
#[test]
fn native_serial_output_reaches_pseudoterminal() -> Result<(), Box<dyn std::error::Error>> {
    use rustix::{fs as osfs, pty, termios};
    let master_fd = pty::openpt(pty::OpenptFlags::RDWR | pty::OpenptFlags::NOCTTY)?;
    pty::grantpt(&master_fd)?;
    pty::unlockpt(&master_fd)?;
    let name = pty::ptsname(&master_fd, Vec::new())?;
    let slave_fd = osfs::open(
        &name,
        osfs::OFlags::RDWR | osfs::OFlags::NOCTTY | osfs::OFlags::NONBLOCK,
        osfs::Mode::empty(),
    )?;
    let mut settings = termios::tcgetattr(&slave_fd)?;
    settings.make_raw();
    termios::tcsetattr(&slave_fd, termios::OptionalActions::Now, &settings)?;
    let mut master = serial2::SerialPort::from(master_fd);
    let slave = serial2::SerialPort::from(slave_fd);
    master.set_read_timeout(Duration::from_millis(500))?;
    // macOS PTYs do not implement the serial hardware speed ioctl. Reuse the
    // OS-created configured port so this tests actual bytes rather than an ioctl mock.
    let mut sinks = OutputSinks::from_serial_port(slave);
    assert!(sinks.write(&row()).is_empty());
    let mut bytes = [0; 4096];
    let n = master.read(&mut bytes)?;
    assert!(std::str::from_utf8(&bytes[..n])?.starts_with("FT, 4, "));
    Ok(())
}
#[test]
fn debug_mosaic_draws_real_maps_and_validates_storage() -> Result<(), Box<dyn std::error::Error>> {
    let views = DebugViews {
        roi: vec![30; 16 * 16],
        diff: vec![60; 16 * 16],
        instant: vec![90; 32 * 16],
        accumulated: vec![120; 32 * 16],
        roi_size: 16,
        map_width: 32,
        map_height: 16,
    };
    let rgb = [10, 20, 30].repeat(32 * 32);
    let out = render_debug_rgb(&rgb, 32, 32, &views, &[[0., 0.], [1., 1.]], 32)?;
    assert_eq!(out.len(), 128 * 96 * 3);
    assert_eq!(&out[(10 * 128 + 70) * 3..(10 * 128 + 70) * 3 + 3], &[30; 3]);
    assert_eq!(&out[(40 * 128 + 70) * 3..(40 * 128 + 70) * 3 + 3], &[90; 3]);
    assert_eq!(
        &out[(70 * 128 + 70) * 3..(70 * 128 + 70) * 3 + 3],
        &[120; 3]
    );
    assert!(render_debug_rgb(&[0], 32, 32, &views, &[], 32).is_err());
    Ok(())
}

#[test]
fn legacy_h264_avi_decodes_same_sample_rgb_as_mp4() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut avi = VideoSource::open(&root.join("tests/fixtures/h264-annexb.avi"))?;
    avi.enable_color();
    let mut mp4 = VideoSource::open(&root.join("../../examples/sample.mp4"))?;
    mp4.enable_color();
    let mut count = 0;
    let mut actual = vec![];
    let mut expected_hash = vec![];
    let hash = |bytes: Vec<u8>| {
        bytes.iter().fold(0xcbf29ce484222325u64, |a, b| {
            (a ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        })
    };
    while let Some(frame) = avi.next_frame()? {
        let expected = mp4.next_frame()?.ok_or("MP4 ended before AVI")?;
        assert!((frame.timestamp_ms() - expected.timestamp_ms()).abs() < 0.001);
        actual.push(hash(avi.take_rgb().ok_or("AVI missing RGB")?));
        expected_hash.push(hash(mp4.take_rgb().ok_or("MP4 missing RGB")?));
        count += 1;
    }
    assert_eq!(
        actual, expected_hash,
        "AVI and MP4 must decode the same pictures in presentation order"
    );
    assert_eq!(count, 10);
    Ok(())
}
#[test]
fn incomplete_recording_remains_partial_and_never_promotes()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("partial")?;
    let final_path = dir.join("incomplete.mp4");
    let mut recorder = VideoRecorder::create(&final_path, 32, 32, 30., RecordingCodec::H264Mp4)?;
    recorder.write_rgb(&[30, 60, 90].repeat(32 * 32))?;
    drop(recorder);
    assert!(!final_path.exists());
    assert!(dir.join("incomplete.mp4.partial").exists());
    assert!(VideoRecorder::create(&final_path, 32, 32, 30., RecordingCodec::H264Mp4).is_err());
    fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn calibrated_debug_input_recenters_off_axis_sphere() -> Result<(), Box<dyn std::error::Error>> {
    use fictrac_core::{Config, Sphere};
    let config = Config {
        sphere: Sphere {
            centre: [0.2, 0., (0.96f64).sqrt()],
            angular_radius: 0.2,
        },
        ..Config::default()
    };
    let views = DebugViews {
        roi: vec![30; 16 * 16],
        diff: vec![60; 16 * 16],
        instant: vec![90; 32 * 16],
        accumulated: vec![120; 32 * 16],
        roi_size: 16,
        map_width: 32,
        map_height: 16,
    };
    let mut rgb = vec![0; 160 * 160 * 3];
    for y in 0..160 {
        for x in 0..160 {
            let i = (y * 160 + x) * 3;
            rgb[i] = x as u8;
            rgb[i + 1] = y as u8;
        }
    }
    let out = fictrac_io::render_calibrated_debug_rgb(&rgb, &config, &views, &[], &[], 64)?;
    // An off-axis centre must sample to the right of the source image centre.
    // Pixel is above arrow origins, inside remapped ball, away from rim/text.
    let red = out[(56 * 256 + 60) * 3];
    assert!(red > 100 && red < 140, "remapped source red={red}");
    assert!(
        fictrac_io::render_calibrated_debug_rgb(
            &rgb,
            &config,
            &views,
            &[],
            &[[f64::NAN, 0., 0.]],
            64
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn default_h264_records_legacy_avi_with_exact_timing_and_pictures()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("legacy-h264")?;
    let path = dir.join("raw.avi");
    let codec = RecordingCodec::from_legacy(None)?;
    assert_eq!(codec, RecordingCodec::H264);
    assert_eq!(codec.extension(), "avi");
    let mut recorder = VideoRecorder::create(&path, 32, 32, 29.97, codec)?;
    for n in 0..5 {
        recorder.write_rgb(&[80 + n * 8, 60, 40].repeat(32 * 32))?;
    }
    recorder.finish()?;
    let mut source = VideoSource::open(&path)?;
    source.enable_color();
    let mut count = 0;
    while let Some(frame) = source.next_frame()? {
        assert!((frame.timestamp_ms() - count as f64 * 1000. / 29.97).abs() < 0.0001);
        let rgb = source.take_rgb().ok_or("missing RGB")?;
        assert!((rgb[(16 * 32 + 16) * 3] as i32 - (80 + count * 8)).abs() < 10);
        count += 1;
    }
    assert_eq!(count, 5);
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn inferred_recording_rate_preserves_prefetched_frames_and_rgb()
-> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample.mp4");
    let mut inferred = VideoSource::open(&path)?;
    inferred.enable_color();
    inferred.info.fps = None;
    let mut ordinary = VideoSource::open(&path)?;
    ordinary.enable_color();
    let rate = inferred.recording_fps()?;
    assert!((rate - 30.).abs() < 1e-8);
    assert_eq!(inferred.recording_fps()?, rate);
    for _ in 0..6 {
        let a = inferred.next_frame()?.ok_or("missing prefetched frame")?;
        let b = ordinary.next_frame()?.ok_or("missing ordinary frame")?;
        assert_eq!(a.timestamp_ms(), b.timestamp_ms());
        assert_eq!(a.pixels(), b.pixels());
        let rgb_a = inferred.take_rgb().ok_or("prefetched RGB lost")?;
        let rgb_b = ordinary.take_rgb().ok_or("ordinary RGB lost")?;
        assert!(rgb_a == rgb_b, "inference must retain exact RGB bytes");
    }
    Ok(())
}
#[test]
fn unknown_single_frame_rate_requires_override_without_losing_frame()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("unknown-rate")?;
    let path = dir.join("single.mp4");
    let mut recorder = VideoRecorder::create(&path, 32, 32, 30., RecordingCodec::H264Mp4)?;
    recorder.write_rgb(&[40, 60, 80].repeat(32 * 32))?;
    recorder.finish()?;
    let mut source = VideoSource::open(&path)?;
    source.info.fps = None;
    assert!(source.recording_fps().is_err());
    assert!(source.next_frame()?.is_some());
    assert!(source.take_rgb().is_some());
    assert!(source.next_frame()?.is_none());
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn legacy_xvid_and_mpg4_encode_real_mpeg4_with_fractional_clock()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("mpeg4")?;
    for name in ["xvid", "mpg4"] {
        let codec = RecordingCodec::from_legacy(Some(name))?;
        let path = dir.join(format!("{name}.{}", codec.extension()));
        let mut writer = VideoRecorder::create(&path, 32, 32, 29.97, codec)?;
        for n in 0..6 {
            let rgb = [80 + n * 8, 60, 40].repeat(32 * 32);
            writer.write_rgb(&rgb)?;
        }
        writer.finish()?;
        let mut video = VideoSource::open(&path)?;
        video.enable_color();
        assert!(video.info.codec.contains("mpeg4"));
        let mut count = 0;
        while let Some(frame) = video.next_frame()? {
            assert!((frame.timestamp_ms() - count as f64 * 1000. / 29.97).abs() < 0.0001);
            let rgb = video.take_rgb().ok_or("missing MPEG4 RGB")?;
            let pixel = (16 * 32 + 16) * 3;
            assert!(
                (rgb[pixel] as i32 - (80 + count * 8)).abs() < 12,
                "{name} frame {count}: red={} expected={}",
                rgb[pixel],
                80 + count * 8
            );
            assert!((rgb[pixel + 1] as i32 - 60).abs() < 12);
            assert!((rgb[pixel + 2] as i32 - 40).abs() < 12);
            count += 1;
        }
        assert_eq!(count, 6, "{name} must preserve every frame");
    }
    fs::remove_dir_all(dir)?;
    Ok(())
}
#[test]
fn mpeg4_records_odd_source_dimensions_without_padding_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("mpeg4-odd")?;
    let file = dir.join("odd.mp4");
    let mut writer = VideoRecorder::create(&file, 31, 29, 30., RecordingCodec::Mpg4)?;
    writer.write_rgb(&[80, 60, 40].repeat(31 * 29))?;
    writer.finish()?;
    let mut source = VideoSource::open(&file)?;
    assert_eq!((source.info.width, source.info.height), (31, 29));
    source.enable_color();
    let frame = source.next_frame()?.ok_or("missing odd frame")?;
    assert_eq!((frame.width(), frame.height()), (31, 29));
    assert_eq!(
        source.take_rgb().ok_or("missing odd RGB")?.len(),
        31 * 29 * 3
    );
    assert!(source.next_frame()?.is_none());
    fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn h264_crops_padded_even_frames_in_avi_and_mp4() -> Result<(), Box<dyn std::error::Error>> {
    let dir = directory("h264-crop")?;
    for codec in [RecordingCodec::H264, RecordingCodec::H264Mp4] {
        let file = dir.join(format!("crop.{}", codec.extension()));
        let mut recorder = VideoRecorder::create(&file, 34, 18, 30., codec)?;
        for n in 0..3 {
            let mut rgb = [80 + n * 8, 60, 40].repeat(34 * 18);
            for y in 0..18 {
                let i = (y * 34 + 33) * 3;
                rgb[i..i + 3].copy_from_slice(&[40, 150, 60]);
            }
            recorder.write_rgb(&rgb)?;
        }
        recorder.finish()?;
        let mut source = VideoSource::open(&file)?;
        source.enable_color();
        assert_eq!((source.info.width, source.info.height), (34, 18));
        let mut count = 0;
        while let Some(frame) = source.next_frame()? {
            assert_eq!((frame.width(), frame.height()), (34, 18));
            assert!((frame.timestamp_ms() - count as f64 * 1000. / 30.).abs() < 0.0001);
            let rgb = source.take_rgb().ok_or("cropped RGB missing")?;
            assert_eq!(rgb.len(), 34 * 18 * 3);
            let center = (8 * 34 + 16) * 3;
            assert!((rgb[center] as i32 - (80 + count * 8)).abs() < 12);
            let edge = (8 * 34 + 33) * 3;
            assert!(
                rgb[edge + 1] > 100,
                "right edge must remain source pixels, not black macroblock padding"
            );
            count += 1;
        }
        assert_eq!(count, 3);
    }
    fs::remove_dir_all(dir)?;
    Ok(())
}
