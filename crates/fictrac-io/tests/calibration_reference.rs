use fictrac_io::{ConfigDocument, capture_calibration_reference, rescale_calibration_reference};

#[test]
fn resolution_reference_scales_pixel_marks_without_changing_legacy_angular_geometry()
-> Result<(), Box<dyn std::error::Error>> {
    let text = "# preserve this\r\nsrc_fn : 0\r\nroi_c : {0,0,1} # angular\r\nroi_r : 0.3\r\nc2a_r : {0,0,0}\r\nroi_circ : {10,20,30,40,50,20}\r\nroi_ignr : {{10,10,20,10,20,20}}\r\nc2a_cnrs_xy : {1,2,3,2,3,4,1,4}\r\n";
    let original = ConfigDocument::parse(text.into())?;
    assert!(rescale_calibration_reference(&original, 200, 200).is_err());
    assert_eq!(original.text(), text);
    let reference = capture_calibration_reference(&original, 100, 100)?;
    assert_eq!(reference.get("roi_circ"), original.get("roi_circ"));
    let resized = rescale_calibration_reference(&reference, 200, 200)?;
    assert_eq!(
        resized.vector::<Vec<f64>>("roi_circ")?,
        vec![20.0, 40.0, 60.0, 80.0, 100.0, 40.0]
    );
    assert_eq!(resized.get("roi_c"), original.get("roi_c"));
    assert_eq!(resized.get("roi_r"), original.get("roi_r"));
    assert_eq!(resized.get("c2a_r"), original.get("c2a_r"));
    assert!(resized.text().contains("# angular\r\n"));
    assert!(rescale_calibration_reference(&reference, 200, 100).is_err());
    let mut stale = reference.clone();
    stale.set("roi_circ", "{1,1,2,2,3,1}")?;
    assert!(rescale_calibration_reference(&stale, 200, 200).is_err());
    let mut changed_fov = reference.clone();
    changed_fov.set("vfov", "50")?;
    assert!(rescale_calibration_reference(&changed_fov, 200, 200).is_err());
    assert!(capture_calibration_reference(&original, usize::MAX, 2).is_err());
    Ok(())
}
