use super::*;
use nokhwa::{
    Camera as NativeCamera,
    pixel_format::RgbFormat,
    utils::{
        ApiBackend, CameraControl, CameraFormat, CameraIndex, ControlValueDescription,
        ControlValueSetter, FrameFormat, KnownCameraControl, KnownCameraControlFlag,
        RequestedFormat, RequestedFormatType, Resolution,
    },
};
use std::time::Instant;
pub fn request_permission() -> Result<()> {
    Ok(())
}
pub fn list_cameras() -> Result<Vec<CameraInfo>> {
    let devices = nokhwa::query(ApiBackend::Auto).map_err(|error| Error(error.to_string()))?;
    #[cfg(target_os = "linux")]
    {
        crate::identity::indexed_camera_infos(
            devices
                .iter()
                .map(|camera| (camera.index().as_string(), camera.human_name())),
        )
    }
    #[cfg(not(target_os = "linux"))]
    {
        devices
            .iter()
            .enumerate()
            .map(|(ordinal, camera)| {
                Ok(CameraInfo {
                    id: u32::try_from(ordinal).map_err(|_| {
                        Error("too many cameras to represent their native ordinal".into())
                    })?,
                    name: camera.human_name(),
                })
            })
            .collect()
    }
}
pub struct Camera {
    camera: NativeCamera,
    start: Instant,
    count: u64,
}
impl Camera {
    /// Identity reported by the opened backend, not an inferred enumeration path.
    pub fn source_label(&self) -> String {
        let info = self.camera.info();
        #[cfg(target_os = "linux")]
        if let Ok(devices) = nokhwa::query(ApiBackend::Video4Linux)
            && let Some(device) = devices
                .iter()
                .find(|device| device.index() == self.camera.index())
            && let Some(path) = device.description().strip_prefix("Video4Linux Device @ ")
            && !path.is_empty()
        {
            return format!("{} ({path})", info.human_name());
        }
        #[cfg(target_os = "windows")]
        if !info.misc().is_empty() {
            return format!("{} ({})", info.human_name(), info.misc());
        }
        format!(
            "{} ({:?} camera {})",
            info.human_name(),
            self.camera.backend(),
            self.camera.index().as_string()
        )
    }
    pub fn open(id: u32, fps: f64) -> Result<Self> {
        if !fps.is_finite() || (fps > 0.0 && !(0.1..=1000.0).contains(&fps)) {
            return Err(Error(
                "camera fps must be finite and 0.1..1000, or nonpositive for automatic".into(),
            ));
        }
        Self::open_internal(
            id,
            CameraFormatRequest {
                width: 640,
                height: 480,
                fps: (fps > 0.0).then_some(fps),
            },
            false,
        )
    }
    pub fn open_with_format(id: u32, request: CameraFormatRequest) -> Result<Self> {
        request.validate()?;
        Self::open_internal(id, request, true)
    }
    fn open_internal(id: u32, request: CameraFormatRequest, exact: bool) -> Result<Self> {
        let requested_fps = request.fps.unwrap_or(30.0);
        if request.fps.is_some() && requested_fps.fract() != 0.0 {
            return Err(Error(
                "this Nokhwa backend supports integer camera FPS only; choose an advertised rate"
                    .into(),
            ));
        }
        let format = RequestedFormat::with_formats(
            RequestedFormatType::Closest(CameraFormat::new(
                Resolution::new(request.width as u32, request.height as u32),
                FrameFormat::YUYV,
                requested_fps as u32,
            )),
            &[
                FrameFormat::YUYV,
                FrameFormat::RAWRGB,
                FrameFormat::RAWBGR,
                FrameFormat::GRAY,
                FrameFormat::NV12,
            ],
        );
        let mut camera =
            NativeCamera::new(CameraIndex::Index(id), format).map_err(|e| Error(e.to_string()))?;
        if exact {
            let selected=camera.compatible_camera_formats().map_err(|e| Error(format!("cannot enumerate camera formats: {e}")))?.into_iter().find(|f|
                f.width() as usize==request.width && f.height() as usize==request.height && raw_format(f.format()) && request.fps.is_none_or(|fps| f64::from(f.frame_rate())==fps)
            ).ok_or_else(|| Error("requested dimensions/FPS are not advertised as an uncompressed camera format; query camera_status formats".into()))?;
            camera
                .set_camera_requset(RequestedFormat::with_formats(
                    RequestedFormatType::Exact(selected),
                    &[selected.format()],
                ))
                .map_err(|e| Error(format!("cannot select camera format: {e}")))?;
        }
        let actual = camera.camera_format();
        CameraFormatRequest {
            width: actual.width() as usize,
            height: actual.height() as usize,
            fps: Some(f64::from(actual.frame_rate())),
        }
        .validate()?;
        if !raw_format(actual.format()) {
            return Err(Error(
                "camera selected a compressed format; choose an uncompressed advertised format"
                    .into(),
            ));
        }
        camera.open_stream().map_err(|e| Error(e.to_string()))?;
        let actual = camera
            .refresh_camera_format()
            .map_err(|e| Error(format!("cannot verify opened camera format: {e}")))?;
        CameraFormatRequest {
            width: actual.width() as usize,
            height: actual.height() as usize,
            fps: Some(f64::from(actual.frame_rate())),
        }
        .validate()?;
        if exact
            && (actual.width() as usize != request.width
                || actual.height() as usize != request.height
                || request
                    .fps
                    .is_some_and(|fps| f64::from(actual.frame_rate()) != fps))
        {
            return Err(Error("camera negotiated a format different from the explicit request; choose another advertised format".into()));
        }
        Ok(Self {
            camera,
            start: Instant::now(),
            count: 0,
        })
    }
    pub fn capabilities(&mut self) -> Result<CameraCapabilities> {
        let active = self
            .camera
            .refresh_camera_format()
            .map_err(|e| Error(format!("cannot read actual camera format: {e}")))?;
        let mut warnings = Vec::new();
        let formats = match self.camera.compatible_camera_formats() {
            Ok(formats) => formats
                .into_iter()
                .filter(|f| raw_format(f.format()))
                .filter_map(|f| {
                    let request = CameraFormatRequest {
                        width: f.width() as usize,
                        height: f.height() as usize,
                        fps: Some(f64::from(f.frame_rate())),
                    };
                    request.validate().ok()?;
                    Some(CameraFormatCapability {
                        width: request.width,
                        height: request.height,
                        pixel_format: format!("{:?}", f.format()),
                        frame_rates: vec![FrameRateRange {
                            min_fps: f64::from(f.frame_rate()),
                            max_fps: f64::from(f.frame_rate()),
                        }],
                    })
                })
                .collect(),
            Err(e) => {
                warnings.push(format!("cannot enumerate supported formats: {e}"));
                Vec::new()
            }
        };
        let exposure = self.exposure_control();
        let exposure_unit = if cfg!(target_os = "windows") {
            "log2(seconds)"
        } else if exposure
            .as_ref()
            .is_ok_and(|c| c.control() == KnownCameraControl::Other(0x009a0902))
        {
            "100 microseconds"
        } else {
            "driver units"
        };
        let exposure_cap = numeric_capability(exposure.as_ref().ok(), exposure_unit);
        let gain = numeric_capability(
            self.camera
                .camera_control(KnownCameraControl::Gain)
                .ok()
                .as_ref(),
            "driver units",
        );
        #[cfg_attr(target_os = "windows", allow(unused_mut))]
        let mut exposure_mode = exposure.as_ref().ok().and_then(|c| {
            if c.flag().contains(&KnownCameraControlFlag::Automatic) {
                Some(ExposureMode::Automatic)
            } else if c.flag().contains(&KnownCameraControlFlag::Manual) {
                Some(ExposureMode::Locked)
            } else {
                None
            }
        });
        #[cfg_attr(target_os = "windows", allow(unused_mut))]
        let mut supported_exposure_modes = Vec::new();
        #[cfg(target_os = "linux")]
        if let Ok(control) = self
            .camera
            .camera_control(KnownCameraControl::Other(0x009a0901))
        {
            if let ControlValueSetter::Integer(value) = control.value() {
                exposure_mode = match value {
                    1 => Some(ExposureMode::Locked),
                    0 | 3 => Some(ExposureMode::Automatic),
                    _ => None,
                };
            }
            let cap = numeric_capability(Some(&control), "V4L2 exposure mode");
            if cap.validate(1.0).is_ok() {
                supported_exposure_modes.push(ExposureMode::Locked);
            }
            if automatic_linux_value(&cap).is_some() {
                supported_exposure_modes.push(ExposureMode::Automatic);
            }
        }
        #[cfg(target_os="windows")]
        warnings.push("Media Foundation/Nokhwa exposes current exposure mode but cannot change automatic/manual flags; numeric exposure/gain remain available when reported writable".into());
        Ok(CameraCapabilities {
            backend: if cfg!(target_os = "windows") {
                "Media Foundation"
            } else {
                "V4L2"
            }
            .into(),
            current_format: CameraFormatRequest {
                width: active.width() as usize,
                height: active.height() as usize,
                fps: Some(f64::from(active.frame_rate())),
            },
            formats,
            exposure_mode,
            supported_exposure_modes,
            exposure: exposure_cap,
            gain,
            warnings,
        })
    }
    fn exposure_control(&self) -> std::result::Result<CameraControl, nokhwa::NokhwaError> {
        #[cfg(target_os = "linux")]
        if let Ok(c) = self
            .camera
            .camera_control(KnownCameraControl::Other(0x009a0902))
        {
            return Ok(c);
        }
        self.camera.camera_control(KnownCameraControl::Exposure)
    }
    pub fn set_control(&mut self, request: CameraControlRequest) -> Result<CameraCapabilities> {
        let (control, value) = match request {
            CameraControlRequest::ExposureMode { mode } => {
                #[cfg(target_os = "windows")]
                {
                    let _ = mode;
                    return Err(Error("Nokhwa Media Foundation cannot change automatic/manual flags; select a numeric exposure/gain control if writable".into()));
                }
                #[cfg(target_os = "linux")]
                {
                    let control = self
                        .camera
                        .camera_control(KnownCameraControl::Other(0x009a0901))
                        .map_err(|e| {
                            Error(format!(
                                "camera has no V4L2 automatic-exposure control: {e}"
                            ))
                        })?;
                    let cap = numeric_capability(Some(&control), "V4L2 exposure mode");
                    let value = match mode {
                        ExposureMode::Locked => 1.0,
                        ExposureMode::Automatic => {
                            automatic_linux_value(&cap).ok_or_else(|| {
                                Error(
                                    "camera does not advertise a supported automatic exposure mode"
                                        .into(),
                                )
                            })?
                        }
                    };
                    cap.validate(value)?;
                    (control, ControlValueSetter::Integer(value as i64))
                }
            }
            CameraControlRequest::Exposure { value } => {
                let control = self
                    .exposure_control()
                    .map_err(|e| Error(format!("camera exposure control unavailable: {e}")))?;
                numeric_capability(Some(&control), "native exposure units").validate(value)?;
                let setter = numeric_setter(&control, value)?;
                (control, setter)
            }
            CameraControlRequest::Gain { value } => {
                let control = self
                    .camera
                    .camera_control(KnownCameraControl::Gain)
                    .map_err(|e| Error(format!("camera gain unavailable: {e}")))?;
                numeric_capability(Some(&control), "driver units").validate(value)?;
                let setter = numeric_setter(&control, value)?;
                (control, setter)
            }
        };
        self.camera
            .set_camera_control(control.control(), value.clone())
            .map_err(|e| Error(format!("camera rejected control: {e}")))?;
        let actual = self
            .camera
            .camera_control(control.control())
            .map_err(|e| Error(format!("cannot read back camera control: {e}")))?;
        if actual.value() != value {
            return Err(Error("camera control readback differs from request; query camera_status for actual settings".into()));
        }
        self.capabilities()
    }
    pub fn next_frame(&mut self) -> Result<Option<CameraFrame>> {
        let frame = self.camera.frame().map_err(|e| Error(e.to_string()))?;
        CameraFormatRequest {
            width: frame.resolution().width() as usize,
            height: frame.resolution().height() as usize,
            fps: None,
        }
        .validate()?;
        let rgb = frame
            .decode_image::<RgbFormat>()
            .map_err(|e| Error(e.to_string()))?;
        let width = rgb.width() as usize;
        let height = rgb.height() as usize;
        let gray = rgb
            .as_raw()
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| {
                (0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2]))
                    / 255.0
            })
            .collect();
        self.count += 1;
        Ok(Some(CameraFrame {
            gray: GrayFrame::new(
                width,
                height,
                gray,
                self.start.elapsed().as_secs_f64() * 1000.0,
            )
            .map_err(|e| Error(e.to_string()))?,
            rgb: rgb.into_raw(),
            sequence: self.count,
        }))
    }
}

fn raw_format(format: FrameFormat) -> bool {
    matches!(
        format,
        FrameFormat::YUYV
            | FrameFormat::RAWRGB
            | FrameFormat::RAWBGR
            | FrameFormat::GRAY
            | FrameFormat::NV12
    )
}
fn numeric_capability(control: Option<&CameraControl>, unit: &str) -> NumericControlCapability {
    let Some(c) = control else {
        return NumericControlCapability::unsupported(
            unit,
            "camera does not expose this control through the native backend",
        );
    };
    let (value, min, max, step) = match *c.description() {
        ControlValueDescription::IntegerRange {
            value,
            min,
            max,
            step,
            ..
        } => (
            value as f64,
            Some(min as f64),
            Some(max as f64),
            step as f64,
        ),
        ControlValueDescription::FloatRange {
            value,
            min,
            max,
            step,
            ..
        } => (value, Some(min), Some(max), step),
        ControlValueDescription::Integer { value, step, .. } => {
            (value as f64, None, None, step as f64)
        }
        ControlValueDescription::Float { value, step, .. } => (value, None, None, step),
        _ => {
            return NumericControlCapability::unsupported(
                unit,
                "camera control is not a numeric control",
            );
        }
    };
    if !value.is_finite()
        || min.is_some_and(|v| !v.is_finite())
        || max.is_some_and(|v| !v.is_finite())
        || min.zip(max).is_some_and(|(min, max)| min > max)
    {
        return NumericControlCapability::unsupported(
            unit,
            "camera supplied invalid numeric control metadata",
        );
    }
    let writable = c.active()
        && !c.flag().iter().any(|f| {
            matches!(
                f,
                KnownCameraControlFlag::ReadOnly
                    | KnownCameraControlFlag::Disabled
                    | KnownCameraControlFlag::Automatic
            )
        });
    NumericControlCapability {
        value: Some(value).filter(|v| v.is_finite()),
        minimum: min,
        maximum: max,
        step: Some(step).filter(|v| v.is_finite() && *v > 0.0),
        unit: unit.into(),
        writable,
        unsupported_reason: (!writable)
            .then(|| "camera control is inactive, automatic, disabled or read-only".into()),
    }
}
fn numeric_setter(control: &CameraControl, value: f64) -> Result<ControlValueSetter> {
    match control.value() {
        ControlValueSetter::Integer(_)
            if value.fract() == 0.0 && value >= i64::MIN as f64 && value < i64::MAX as f64 =>
        {
            Ok(ControlValueSetter::Integer(value as i64))
        }
        ControlValueSetter::Float(_) => Ok(ControlValueSetter::Float(value)),
        _ => Err(Error(
            "camera requires an exactly representable native numeric control value".into(),
        )),
    }
}
#[cfg(target_os = "linux")]
fn automatic_linux_value(cap: &NumericControlCapability) -> Option<f64> {
    // V4L2 menus may omit entries: only use automatic entries at an advertised endpoint.
    [0.0, 3.0].into_iter().find(|value| {
        (cap.minimum == Some(*value) || cap.maximum == Some(*value)) && cap.validate(*value).is_ok()
    })
}
