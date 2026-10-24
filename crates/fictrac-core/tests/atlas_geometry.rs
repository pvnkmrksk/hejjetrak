use fictrac_core::{Config, Diagnostics, GrayFrame};

#[test]
fn small_odd_source_uses_legacy_roi_and_even_atlas_dimensions() -> fictrac_core::Result<()> {
    let config = Config {
        width: 41,
        height: 64,
        ..Config::default()
    };
    let mut diagnostics = Diagnostics::new(config, 6, 1.25, 0.25)?;
    let views = diagnostics.update(&GrayFrame::new(41, 64, vec![0.5; 41 * 64], 0.0)?, None);
    assert_eq!(views.roi_size, 41);
    assert_eq!((views.map_width, views.map_height), (122, 61));
    assert_eq!(views.accumulated.len(), 122 * 61);
    assert_eq!(views.instant.len(), 122 * 61);
    Ok(())
}
