use fictrac_core::{Config, Diagnostics, Geometry, GrayFrame, Sphere};

#[test]
fn wide_angle_off_axis_sphere_only_intersects_forward_ray_half_line() -> fictrac_core::Result<()> {
    let config = Config {
        width: 320,
        height: 160,
        vfov_deg: 169.,
        sphere: Sphere {
            centre: glam::DVec3::new(-1., 0., 0.05).normalize().to_array(),
            angular_radius: 0.10,
        },
        ..Config::default()
    };
    let geometry = Geometry::new(config.clone())?;
    let visible = geometry
        .surface(1., 80.)
        .ok_or_else(|| fictrac_core::Error::Config("visible test sphere is missing".into()))?;
    let point = glam::DVec3::from_array(config.sphere.centre)
        + visible * config.sphere.angular_radius.sin();
    assert!(point.z > 0.);
    let projected = geometry
        .project(visible)
        .ok_or_else(|| fictrac_core::Error::Config("visible test surface cannot project".into()))?;
    assert!((projected[0] - 1.).abs() < 1e-9);
    assert!((projected[1] - 80.).abs() < 1e-9);
    // The opposite image edge intersects the same infinite line behind the
    // camera, but that is not a surface visible along this pixel's forward ray.
    assert!(geometry.surface(319., 80.).is_none());
    Ok(())
}

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
