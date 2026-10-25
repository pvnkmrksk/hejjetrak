use fictrac_io::{ConfigDocument, load_config_text};
type Result = std::result::Result<(), Box<dyn std::error::Error>>;
fn fixture() -> std::result::Result<ConfigDocument, Box<dyn std::error::Error>> {
    Ok(ConfigDocument::parse(
        include_str!("../../../examples/upstream-config.txt").into(),
    )?)
}
#[test]
fn document_booleans_agree_with_legacy_projection_and_reference_metadata() -> Result {
    for value in ["y", "Y", "1", "true", "n", "N", "0", "false"] {
        let mut doc = fixture()?;
        doc.set("fisheye", value)?;
        let loaded = load_config_text(doc.text(), 384, 288)?;
        assert_eq!(doc.boolean("fisheye"), loaded.config.fisheye, "{value}");
        let reference = fictrac_io::capture_calibration_reference(&doc, 384, 288)?;
        assert_eq!(
            reference.boolean("rust_calibration_fisheye"),
            loaded.config.fisheye,
            "{value}"
        );
        let mut changed = reference;
        changed.set("fisheye", if loaded.config.fisheye { "n" } else { "y" })?;
        assert!(fictrac_io::rescale_calibration_reference(&changed, 768, 576).is_err());
    }
    Ok(())
}
#[test]
fn runtime_and_recovery_parameters_are_applied_or_recognized() -> Result {
    let mut doc = fixture()?;
    for (key, value) in [
        ("opt_do_global", "Y"),
        ("max_bad_frames", "3"),
        ("opt_global_evals", "4000"),
        ("src_fps", "60"),
        ("sphere_map_fn", "template.png"),
        ("save_debug", "y"),
        ("vid_codec", "h264"),
        ("com_port", "test-port"),
        ("com_baud", "115200"),
    ] {
        doc.set(key, value)?;
    }
    let loaded = load_config_text(doc.text(), 384, 288)?;
    let map = loaded.config.spherical_map.ok_or("missing map settings")?;
    assert!(map.global_search);
    assert_eq!(map.max_bad_frames, 3);
    assert_eq!(map.global_evaluations, 4000);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    doc.set("src_fps", "1e999")?;
    assert!(load_config_text(doc.text(), 384, 288).is_err());
    Ok(())
}
#[test]
fn corner_only_legacy_calibration_uses_shared_square_solver() -> Result {
    let original = fixture()?;
    let text = original
        .text()
        .lines()
        .filter(|line| !line.trim_start().starts_with("c2a_r "))
        .collect::<Vec<_>>()
        .join("\n");
    let doc = ConfigDocument::parse(text)?;
    let loaded = load_config_text(doc.text(), 384, 288)?;
    let corners: Vec<f64> = doc.vector("c2a_cnrs_xy")?;
    let expected = fictrac_core::calibration::lab_from_square(
        384,
        288,
        45.0,
        false,
        "xy",
        corners.as_chunks::<2>().0,
    )?
    .0;
    assert_eq!(loaded.config.camera_to_lab, expected);
    assert!(expected.iter().all(|v| v.is_finite()));
    Ok(())
}
