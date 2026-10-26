use crate::CameraFormatRequest;

/// An advertised format, independent of a particular OS capture library.
#[derive(Clone, Copy)]
pub(crate) struct FormatCandidate {
    pub width: usize,
    pub height: usize,
    pub fps: f64,
    pub compressed: bool,
}

/// Choose only validated advertised formats. Pixel encoding may differ from
/// YUYV: mono and MJPEG-only cameras are equally valid capture sources.
pub(crate) fn select_format(
    formats: &[FormatCandidate],
    request: &CameraFormatRequest,
    exact: bool,
) -> Option<usize> {
    let wanted_fps = request.fps.unwrap_or(30.0);
    formats
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            CameraFormatRequest {
                width: f.width,
                height: f.height,
                fps: Some(f.fps),
            }
            .validate()
            .is_ok()
                && (!exact
                    || (f.width == request.width
                        && f.height == request.height
                        && request.fps.is_none_or(|fps| fps == f.fps)))
        })
        .min_by(|(_, a), (_, b)| {
            let distance = |f: &FormatCandidate| {
                let dx = f.width.abs_diff(request.width) as u128;
                let dy = f.height.abs_diff(request.height) as u128;
                dx * dx + dy * dy
            };
            distance(a)
                .cmp(&distance(b))
                .then_with(|| {
                    (a.fps - wanted_fps)
                        .abs()
                        .total_cmp(&(b.fps - wanted_fps).abs())
                })
                .then_with(|| a.compressed.cmp(&b.compressed))
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mono_and_mjpeg_sources_do_not_need_a_yuyv_mode_or_exact_default_size() {
        let request = CameraFormatRequest {
            width: 640,
            height: 480,
            fps: None,
        };
        // A mono sensor often has 640x400 rather than the default 640x480.
        let formats = [
            FormatCandidate {
                width: 1280,
                height: 800,
                fps: 120.0,
                compressed: false,
            },
            FormatCandidate {
                width: 640,
                height: 400,
                fps: 100.0,
                compressed: false,
            },
        ];
        assert_eq!(select_format(&formats, &request, false), Some(1));
        assert_eq!(select_format(&formats, &request, true), None);
        let mjpeg_only = [FormatCandidate {
            width: 640,
            height: 480,
            fps: 30.0,
            compressed: true,
        }];
        assert_eq!(select_format(&mjpeg_only, &request, false), Some(0));
    }

    #[test]
    fn exact_requests_and_bounds_survive_automatic_format_selection() {
        let request = CameraFormatRequest {
            width: 640,
            height: 480,
            fps: Some(30.0),
        };
        let formats = [
            FormatCandidate {
                width: usize::MAX,
                height: 480,
                fps: 30.0,
                compressed: false,
            },
            FormatCandidate {
                width: 640,
                height: 480,
                fps: f64::NAN,
                compressed: false,
            },
            FormatCandidate {
                width: 640,
                height: 480,
                fps: 30.0,
                compressed: true,
            },
            FormatCandidate {
                width: 640,
                height: 480,
                fps: 30.0,
                compressed: false,
            },
            FormatCandidate {
                width: 640,
                height: 480,
                fps: 60.0,
                compressed: false,
            },
        ];
        assert_eq!(select_format(&formats, &request, true), Some(3));
        assert_eq!(select_format(&formats, &request, false), Some(3));
        let unknown_rate = CameraFormatRequest {
            fps: None,
            ..request
        };
        assert_eq!(select_format(&formats, &unknown_rate, true), Some(3));
    }
}
