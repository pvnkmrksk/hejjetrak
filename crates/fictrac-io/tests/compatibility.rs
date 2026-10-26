use fictrac_core::Tracker;
use fictrac_io::{ConfigDocument, DocumentHistory, VideoSource, load_config_text};
use std::{fs, path::PathBuf};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
#[test]
fn all_legacy_fixtures_round_trip_byte_exact() -> Result {
    for path in [
        root().join("examples/upstream-config.txt"),
        root().join("examples/benchmark/config.txt"),
        root().join("examples/rigs/BR1/config.txt"),
        root().join("examples/rigs/BR2/config.txt"),
        root().join("examples/rigs/BR3/config.txt"),
        root().join("examples/rigs/BR4/config.txt"),
    ] {
        let bytes = fs::read_to_string(&path)?;
        let doc = ConfigDocument::parse(bytes.clone())?;
        assert_eq!(doc.text(), bytes);
        let mut changed = doc.clone();
        changed.set("vfov", "66.0000")?;
        changed.set("vfov", doc.get("vfov").ok_or("missing vfov")?)?;
        assert_eq!(
            changed.text(),
            bytes,
            "{} changed on round trip",
            path.display()
        );
    }
    Ok(())
}
#[test]
fn comments_unknown_parameters_crlf_and_undo_are_preserved() -> Result {
    let original = "# instructions here are data\r\nsrc_fn   : 0  # device\r\nvfov:45\r\nfuture_extension : { 1, 2 }\r\nempty :\r\n";
    let mut doc = ConfigDocument::parse(original.into())?;
    doc.set("src_fn", "1")?;
    doc.set("empty", "hello")?;
    assert!(doc.text().contains("src_fn   : 1  # device\r\n"));
    assert!(doc.text().contains("empty :hello\r\n"));
    assert_eq!(doc.get("future_extension"), Some("{ 1, 2 }"));
    let mut history = DocumentHistory::new(original.into());
    history.replace(doc.text().into());
    assert!(history.undo());
    assert_eq!(history.text, original);
    assert!(history.redo());
    assert_eq!(history.text, doc.text());
    Ok(())
}
#[test]
fn instances_select_next_rig_and_increment_only_camera_sources() -> Result {
    let path = root().join("examples/rigs/BR1/config.txt");
    let doc = ConfigDocument::parse(fs::read_to_string(&path)?)?;
    let (next, path) = doc.next_instance(Some(&path))?;
    assert_eq!(next.get("src_fn"), Some("2"));
    assert!(path.is_some_and(|p| p.ends_with("BR2/config.txt")));
    let (copy, path) = next.next_instance(None)?;
    assert_eq!(copy.get("src_fn"), Some("3"));
    assert!(path.is_none());
    let file = ConfigDocument::parse("src_fn : sample.mp4\n".into())?;
    let (copy, _) = file.next_instance(None)?;
    assert_eq!(copy.text(), file.text());
    Ok(())
}

#[test]
fn saved_rig_selection_wraps_and_skips_missing_slots() -> Result {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "hejjetrac-rig-cycle-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&directory)?;
    for index in [1, 3, 4] {
        let text = format!("# rig {index}\nsrc_fn : {}\n", (index - 1) * 2);
        fs::write(directory.join(format!("VR{index}.txt")), &text)?;
        let folder = directory.join(format!("VR{index}"));
        fs::create_dir(&folder)?;
        fs::write(folder.join("config.txt"), text)?;
    }
    for start in [directory.join("VR3.txt"), directory.join("VR3/config.txt")] {
        let mut path = start;
        let mut document = ConfigDocument::parse(fs::read_to_string(&path)?)?;
        for expected in ["6", "0", "4"] {
            let (next, selected) = document.next_instance(Some(&path))?;
            assert_eq!(next.get("src_fn"), Some(expected));
            path = selected.ok_or("saved sibling was replaced by an uncalibrated camera copy")?;
            assert_eq!(next.text(), fs::read_to_string(&path)?);
            document = next;
        }
    }
    // Ordinary numbered experiments keep their existing forward-only fallback.
    fs::write(directory.join("trial1.txt"), "src_fn:99\n")?;
    let document = ConfigDocument::parse("src_fn:41\n".into())?;
    let (copy, path) = document.next_instance(Some(&directory.join("trial9.txt")))?;
    assert!(path.is_none());
    assert_eq!(copy.get("src_fn"), Some("42"));
    fs::remove_dir_all(directory)?;
    Ok(())
}
#[test]
fn saved_roi_has_precedence_over_rim_refitting() -> Result {
    let path = root().join("examples/rigs/BR1/config.txt");
    let doc = ConfigDocument::parse(fs::read_to_string(&path)?)?;
    let loaded = load_config_text(doc.text(), 320, 240)?;
    assert!((loaded.config.sphere.angular_radius - 0.140809).abs() < 1e-12);
    assert_eq!(
        loaded.config.sphere.centre,
        [-0.014917, -0.021012, 0.999668]
    );
    assert!(loaded.config.legacy_roi_coordinates);
    Ok(())
}
#[test]
fn sample_motion_stays_near_user_supplied_upstream_reference() -> Result {
    let reference = fs::read_to_string(root().join("examples/benchmark/upstream.dat"))?;
    let reference: Vec<Vec<f64>> = reference
        .lines()
        .map(|line| line.split(',').map(|s| s.trim().parse::<f64>()).collect())
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(reference.len(), 300);
    let mut doc = ConfigDocument::parse(fs::read_to_string(
        root().join("examples/benchmark/config.txt"),
    )?)?;
    doc.set("rust_engine", "pairwise")?;
    let config = load_config_text(doc.text(), 384, 288)?.config;
    let mut tracker = Tracker::new(config)?;
    let mut video = VideoSource::open(&root().join("examples/sample.mp4"))?;
    let mut n = 0;
    let mut squared = [0.0; 3];
    let mut path_squared = 0.0;
    let mut heading_squared = 0.0;
    let mut last = [0.0; 2];
    let mut last_heading = 0.0;
    while let Some(frame) = video.next_frame()? {
        let row = tracker.process(frame)?;
        let expected = reference.get(n).ok_or("extra decoded frame")?;
        assert_eq!(expected.len(), 25);
        assert_eq!(row.frame, n as u64);
        assert!(row.accepted);
        if n > 0 {
            for axis in 0..3 {
                squared[axis] += (row.delta_camera[axis] - expected[axis + 1]).powi(2);
            }
            path_squared +=
                (row.position[0] - expected[14]).powi(2) + (row.position[1] - expected[15]).powi(2);
            let d = row.heading - expected[16];
            heading_squared += d.sin().atan2(d.cos()).powi(2);
        }
        last = row.position;
        last_heading = row.heading;
        n += 1;
    }
    assert_eq!(n, 300);
    let rotation = squared.map(|v| (v / 299.0).sqrt());
    let path_rmse = (path_squared / 299.0).sqrt();
    let heading_rmse = (heading_squared / 299.0).sqrt();
    let endpoint = (last[0] - reference[299][14]).hypot(last[1] - reference[299][15]);
    eprintln!(
        "upstream comparison: delta component RMSE {rotation:?} rad; path point RMSE {path_rmse} rad; heading RMSE {heading_rmse} rad; endpoint error {endpoint} rad; final {last:?}, heading {last_heading}"
    );
    // Declared first comparison gates, not a claim of numerical equivalence or physical ground truth.
    assert!(
        rotation.iter().all(|v| *v < 0.012),
        "rotation regression {rotation:?}"
    );
    assert!(path_rmse < 0.40, "path regression {path_rmse}");
    assert!(heading_rmse < 0.15, "heading regression {heading_rmse}");
    assert!(endpoint < 0.50, "endpoint regression {endpoint}");
    Ok(())
}

#[test]
fn spherical_backend_matches_the_reference_path_and_heading() -> Result {
    let root = root();
    let config = load_config_text(
        &fs::read_to_string(root.join("examples/benchmark/config.txt"))?,
        384,
        288,
    )?
    .config;
    let channel = config
        .spherical_map
        .as_ref()
        .ok_or("legacy map backend missing")?
        .channel;
    let mut tracker = Tracker::new(config)?;
    let mut video = VideoSource::open(&root.join("examples/sample.mp4"))?;
    let mut bytes = vec![];
    while let Some(frame) = video.next_tracking_frame(channel)? {
        let row = tracker.process(frame)?;
        assert!(row.accepted);
        fictrac_io::write_dat(&mut bytes, &row)?;
    }
    let comparison = fictrac_io::compare_dat(
        std::io::BufReader::new(fs::File::open(
            root.join("examples/benchmark/upstream.dat"),
        )?),
        std::io::Cursor::new(bytes),
    )?;
    eprintln!(
        "spherical-map benchmark: {}",
        serde_json::to_string_pretty(&comparison)?
    );
    assert!(
        comparison.passes(0.075, 0.025, 0.020),
        "map backend no longer matches its declared sample benchmark"
    );
    assert!(comparison.endpoint_error_sphere_rad < 0.075);
    Ok(())
}
