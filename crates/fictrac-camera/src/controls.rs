//! Serializable native camera contracts. Units and availability are device/backend specific.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraFormatRequest {
    pub width: usize,
    pub height: usize,
    pub fps: Option<f64>,
}
impl CameraFormatRequest {
    pub fn validate(&self) -> Result<()> {
        if !(16..=8192).contains(&self.width)
            || !(16..=8192).contains(&self.height)
            || self
                .width
                .checked_mul(self.height)
                .is_none_or(|n| n > 8_000_000)
        {
            return Err(Error(
                "camera dimensions must be 16..8192 and at most 8 million pixels".into(),
            ));
        }
        if self
            .fps
            .is_some_and(|fps| !fps.is_finite() || !(0.1..=1000.0).contains(&fps))
        {
            return Err(Error(
                "camera fps must be finite and 0.1..1000, or null for automatic".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameRateRange {
    pub min_fps: f64,
    pub max_fps: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraFormatCapability {
    pub width: usize,
    pub height: usize,
    pub pixel_format: String,
    pub frame_rates: Vec<FrameRateRange>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericControlCapability {
    pub value: Option<f64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub step: Option<f64>,
    pub unit: String,
    pub writable: bool,
    pub unsupported_reason: Option<String>,
}
impl NumericControlCapability {
    pub fn unsupported(unit: &str, reason: &str) -> Self {
        Self {
            value: None,
            minimum: None,
            maximum: None,
            step: None,
            unit: unit.into(),
            writable: false,
            unsupported_reason: Some(reason.into()),
        }
    }
    /// Validate before an OS mutation. A missing range is deliberately not invented.
    pub fn validate(&self, value: f64) -> Result<()> {
        if !self.writable {
            return Err(Error(
                self.unsupported_reason
                    .clone()
                    .unwrap_or_else(|| "camera control is read-only".into()),
            ));
        }
        if self.minimum.is_some_and(|v| !v.is_finite())
            || self.maximum.is_some_and(|v| !v.is_finite())
            || self.step.is_some_and(|v| !v.is_finite() || v <= 0.0)
            || self
                .minimum
                .zip(self.maximum)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(Error(
                "camera supplied invalid control range/step metadata".into(),
            ));
        }
        if !value.is_finite()
            || self.minimum.is_some_and(|v| value < v)
            || self.maximum.is_some_and(|v| value > v)
        {
            return Err(Error(format!(
                "camera value {value} is outside reported {:?}..{:?} {}",
                self.minimum, self.maximum, self.unit
            )));
        }
        if let (Some(step), Some(origin)) = (self.step, self.minimum.or(self.value)) {
            let ticks = (value - origin) / step;
            if (ticks - ticks.round()).abs() > 1e-6 {
                return Err(Error(format!(
                    "camera value must follow step {step} {}",
                    self.unit
                )));
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureMode {
    Automatic,
    Locked,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraCapabilities {
    pub backend: String,
    pub current_format: CameraFormatRequest,
    pub formats: Vec<CameraFormatCapability>,
    pub exposure_mode: Option<ExposureMode>,
    pub supported_exposure_modes: Vec<ExposureMode>,
    pub exposure: NumericControlCapability,
    pub gain: NumericControlCapability,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "control", rename_all = "snake_case", deny_unknown_fields)]
pub enum CameraControlRequest {
    ExposureMode { mode: ExposureMode },
    Exposure { value: f64 },
    Gain { value: f64 },
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_round_trip_rejects_unknown_fields()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let request = CameraControlRequest::ExposureMode {
            mode: ExposureMode::Locked,
        };
        assert_eq!(
            serde_json::to_string(&request)?,
            r#"{"control":"exposure_mode","mode":"locked"}"#
        );
        assert_eq!(
            serde_json::from_str::<CameraControlRequest>(&serde_json::to_string(&request)?)?,
            request
        );
        assert!(
            serde_json::from_str::<CameraControlRequest>(
                r#"{"control":"gain","value":1,"unexpected":true}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<CameraFormatRequest>(
                r#"{"width":640,"height":480,"fps":30,"other":1}"#
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn format_and_numeric_validation() -> Result<()> {
        let mut f = CameraFormatRequest {
            width: 640,
            height: 480,
            fps: Some(30.0),
        };
        f.validate()?;
        for fps in [f64::NAN, f64::INFINITY, 0.0, -1.0, 1000.1] {
            f.fps = Some(fps);
            assert!(f.validate().is_err());
        }
        f.fps = None;
        f.width = usize::MAX;
        assert!(f.validate().is_err());
        let c = NumericControlCapability {
            value: Some(2.0),
            minimum: Some(1.0),
            maximum: Some(9.0),
            step: Some(2.0),
            unit: "driver units".into(),
            writable: true,
            unsupported_reason: None,
        };
        c.validate(5.0)?;
        let unknown_minimum = NumericControlCapability {
            minimum: None,
            value: Some(1.0),
            ..c.clone()
        };
        unknown_minimum.validate(3.0)?;
        assert!(unknown_minimum.validate(2.0).is_err());
        let mut malformed = c.clone();
        malformed.minimum = Some(f64::NAN);
        assert!(malformed.validate(5.0).is_err());
        malformed = c.clone();
        malformed.step = Some(-1.0);
        assert!(malformed.validate(5.0).is_err());
        for v in [0.0, 2.0, 10.0, f64::NAN] {
            assert!(c.validate(v).is_err());
        }
        assert!(
            NumericControlCapability::unsupported("ISO", "unavailable")
                .validate(100.0)
                .is_err()
        );
        Ok(())
    }
}
