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
fn omitted_precision_fields_use_shared_robust_defaults_and_explicit_values_are_preserved() -> Result
{
    let original = fixture()?;
    let text = original
        .text()
        .lines()
        .filter(|line| {
            let key = line.split(':').next().unwrap_or("").trim();
            !matches!(key, "q_factor" | "opt_max_evals")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let loaded = load_config_text(&text, 384, 288)?;
    let map = loaded.config.spherical_map.as_ref().ok_or("missing map")?;
    assert_eq!(map.quality, 6);
    assert_eq!(map.max_evaluations, 100);
    assert_eq!(map.resilient_error_floor, 10_000.);
    let defaults = fictrac_core::MapSettings::default();
    assert_eq!(map.quality, defaults.quality);
    assert_eq!(map.max_evaluations, defaults.max_evaluations);
    let mut explicit = ConfigDocument::parse(text)?;
    explicit.set("q_factor", "6")?;
    explicit.set("opt_max_evals", "50")?;
    let map = load_config_text(explicit.text(), 384, 288)?
        .config
        .spherical_map
        .ok_or("missing explicit map")?;
    assert_eq!(map.quality, 6);
    assert_eq!(map.max_evaluations, 50);
    Ok(())
}

#[test]
fn resilient_error_floor_round_trips_legacy_and_json_with_existing_default() -> Result {
    let original = fixture()?;
    let loaded = load_config_text(original.text(), 384, 288)?;
    assert_eq!(
        loaded
            .config
            .spherical_map
            .as_ref()
            .ok_or("missing map")?
            .resilient_error_floor,
        10_000.0
    );
    for floor in ["10000", "15000", "18000"] {
        let mut doc = original.clone();
        doc.set("rust_resilient_error_floor", floor)?;
        let loaded = load_config_text(doc.text(), 384, 288)?;
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let value: f64 = floor.parse()?;
        assert_eq!(
            loaded
                .config
                .spherical_map
                .as_ref()
                .ok_or("missing map")?
                .resilient_error_floor,
            value
        );
        let json = serde_json::to_string(&loaded.config)?;
        let json_loaded = load_config_text(&json, 384, 288)?;
        assert_eq!(
            json_loaded
                .config
                .spherical_map
                .as_ref()
                .ok_or("missing map")?
                .resilient_error_floor,
            value
        );
    }
    let mut json = serde_json::to_value(&loaded.config)?;
    if let Some(map) = json["spherical_map"].as_object_mut() {
        map.remove("resilient_error_floor");
    }
    let old_json = load_config_text(&serde_json::to_string(&json)?, 384, 288)?;
    assert_eq!(
        old_json
            .config
            .spherical_map
            .ok_or("missing map")?
            .resilient_error_floor,
        10_000.0
    );
    let (category, description) = fictrac_io::parameter_description("rust_resilient_error_floor");
    assert_eq!(category, "Tracking");
    assert!(description.contains("10000..18000") && description.contains("hard cap"));
    Ok(())
}

#[test]
fn resilient_error_floor_rejects_out_of_bounds_and_nonfinite_configuration() -> Result {
    let original = fixture()?;
    for floor in ["9999", "18001", "1e999", "null", "\"NaN\""] {
        let mut doc = original.clone();
        doc.set("rust_resilient_error_floor", floor)?;
        assert!(
            load_config_text(doc.text(), 384, 288).is_err(),
            "invalid legacy floor {floor}"
        );
    }
    let loaded = load_config_text(original.text(), 384, 288)?;
    for floor in [
        serde_json::json!(9999),
        serde_json::json!(18001),
        serde_json::Value::Null,
        serde_json::json!("NaN"),
    ] {
        let mut json = serde_json::to_value(&loaded.config)?;
        json["spherical_map"]["resilient_error_floor"] = floor;
        assert!(load_config_text(&serde_json::to_string(&json)?, 384, 288).is_err());
    }
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
