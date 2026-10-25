//! Preserve native V4L indices when device numbers contain holes.
use crate::{CameraInfo, Error, Result};

pub(crate) fn indexed_camera_infos(
    devices: impl IntoIterator<Item = (String, String)>,
) -> Result<Vec<CameraInfo>> {
    devices
        .into_iter()
        .map(|(index, name)| {
            let id = index.parse::<u32>().map_err(|_| {
                Error(format!(
                    "native V4L camera index {index:?} is not numeric; the camera chooser requires a backend exposing numeric V4L device IDs"
                ))
            })?;
            Ok(CameraInfo { id, name })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gapped_linux_devices_keep_actual_ids_and_names() -> Result<()> {
        let cameras = indexed_camera_infos([
            ("0".into(), "built-in".into()),
            ("2".into(), "left rig".into()),
            ("7".into(), "right rig".into()),
        ])?;
        assert_eq!(
            cameras.iter().map(|camera| camera.id).collect::<Vec<_>>(),
            [0, 2, 7]
        );
        assert_eq!(cameras[2].name, "right rig");
        Ok(())
    }

    #[test]
    fn unsupported_string_device_ids_never_become_fabricated_ordinals() {
        let error = indexed_camera_infos([
            ("0".into(), "built-in".into()),
            ("not-a-numeric-v4l-device".into(), "external".into()),
        ])
        .err();
        assert!(error.is_some_and(|error| error.to_string().contains("not numeric")));
    }
}
