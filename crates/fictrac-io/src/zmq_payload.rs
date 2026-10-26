//! Explicit live-output compatibility. The numerical engine keeps sphere radians.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const DEFAULT_ZMQ_TOPIC: &str = "FT";
const MAX_TOPIC_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZmqOutputMode {
    #[default]
    NativeFt,
    LegacyMatrexVr,
}
impl ZmqOutputMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeFt => "native_ft",
            Self::LegacyMatrexVr => "legacy_matrex_vr",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZmqFraming {
    #[default]
    Single,
    TopicJson,
}
impl ZmqFraming {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::TopicJson => "topic_json",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZmqPayloadSettings {
    pub mode: ZmqOutputMode,
    pub framing: ZmqFraming,
    pub topic: String,
}
impl Default for ZmqPayloadSettings {
    fn default() -> Self {
        Self {
            mode: ZmqOutputMode::NativeFt,
            framing: ZmqFraming::Single,
            topic: DEFAULT_ZMQ_TOPIC.into(),
        }
    }
}
impl ZmqPayloadSettings {
    pub fn parse(mode: Option<&str>, framing: Option<&str>, topic: Option<&str>) -> Result<Self> {
        let mode = match mode {
            None | Some("native_ft") => ZmqOutputMode::NativeFt,
            Some("legacy_matrex_vr") => ZmqOutputMode::LegacyMatrexVr,
            Some(_) => {
                return Err(Error::Config(
                    "zmq_output_mode must be native_ft or legacy_matrex_vr".into(),
                ));
            }
        };
        let framing = match framing {
            None | Some("single") => ZmqFraming::Single,
            Some("topic_json") => ZmqFraming::TopicJson,
            Some(_) => {
                return Err(Error::Config(
                    "zmq_framing must be single or topic_json".into(),
                ));
            }
        };
        let topic = topic.unwrap_or(DEFAULT_ZMQ_TOPIC);
        if topic.len() > MAX_TOPIC_BYTES || topic.contains(['\0', '\r', '\n']) {
            return Err(Error::Config(
                "zmq_topic must be at most 256 UTF-8 bytes without NUL or line breaks".into(),
            ));
        }
        Ok(Self {
            mode,
            framing,
            topic: topic.into(),
        })
    }

    pub fn effective_framing(&self) -> ZmqFraming {
        if self.mode == ZmqOutputMode::NativeFt {
            ZmqFraming::Single
        } else {
            self.framing
        }
    }

    /// Preserve a native FT record exactly, or reproduce the Python MATREX
    /// republisher from that record's already-rounded comma-split fields.
    /// An empty topic is valid ZMQ framing. Native FT always uses one frame.
    pub(crate) fn encode(&self, native_ft: Vec<u8>) -> Result<Vec<Vec<u8>>> {
        if self.mode == ZmqOutputMode::NativeFt {
            return Ok(vec![native_ft]);
        }
        let fields = legacy_fields(&native_ft)?;
        let yaw = fields[2].to_degrees();
        if !yaw.is_finite() {
            return Err(Error::Config(
                "legacy MATREX output rejects nonfinite yaw after radians-to-degrees conversion"
                    .into(),
            ));
        }
        let json = format!(
            "{{\"x\": {}, \"y\": {}, \"z\": 0.0, \"yaw\": {}, \"pitch\": 0.0, \"roll\": 0.0}}",
            python_float(fields[0])?,
            python_float(fields[1])?,
            python_float(yaw)?,
        )
        .into_bytes();
        Ok(match self.framing {
            ZmqFraming::Single => vec![json],
            ZmqFraming::TopicJson => vec![self.topic.as_bytes().to_vec(), json],
        })
    }
}

fn legacy_fields(packet: &[u8]) -> Result<[f64; 3]> {
    let text = std::str::from_utf8(packet)
        .map_err(|_| Error::Config("legacy MATREX output requires a UTF-8 FT row".into()))?;
    let mut fields = text.split(',');
    if fields.next().map(str::trim) != Some("FT") {
        return Err(Error::Config(
            "legacy MATREX output requires the FT prefix at field zero".into(),
        ));
    }
    let mut selected = [0.0; 3];
    let mut count = 0;
    for (index, field) in fields.enumerate() {
        // `index + 1` includes FT at index zero, as in the Python bridge.
        let value = field.trim().parse::<f64>().map_err(|_| {
            Error::Config(format!(
                "legacy MATREX output requires numeric FT field {}",
                index + 1
            ))
        })?;
        if !value.is_finite() {
            return Err(Error::Config(format!(
                "legacy MATREX output rejects nonfinite FT field {}",
                index + 1
            )));
        }
        if (14..=16).contains(&index) {
            selected[index - 14] = value;
        }
        count += 1;
    }
    if count != 25 {
        return Err(Error::Config(
            "legacy MATREX output requires exactly 25 numeric FT fields".into(),
        ));
    }
    Ok(selected)
}

/// Python's json.dumps uses float repr: shortest round-trip digits, a decimal
/// point for fixed integers, scientific notation below 1e-4 or from 1e16, and
/// a signed exponent with at least two digits. Preserve signed zero as well.
fn python_float(value: f64) -> Result<String> {
    if !value.is_finite() {
        return Err(Error::Config(
            "legacy MATREX JSON cannot publish a nonfinite value".into(),
        ));
    }
    let number = serde_json::to_string(&value)?;
    let (sign, unsigned) = number
        .strip_prefix('-')
        .map_or(("", number.as_str()), |rest| ("-", rest));
    let (mantissa, explicit_exponent) = match unsigned.split_once('e') {
        Some((mantissa, exponent)) => (
            mantissa,
            exponent
                .parse::<i32>()
                .map_err(|_| Error::Config("cannot format legacy MATREX JSON exponent".into()))?,
        ),
        None => (unsigned, 0),
    };
    let decimal_position = mantissa.find('.').unwrap_or(mantissa.len()) as i32;
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let Some(first) = digits.bytes().position(|digit| digit != b'0') else {
        return Ok(format!("{sign}0.0"));
    };
    let exponent = decimal_position - 1 - first as i32 + explicit_exponent;
    let significant = digits[first..].trim_end_matches('0');
    if !(-4..16).contains(&exponent) {
        let mantissa = if significant.len() == 1 {
            significant.to_owned()
        } else {
            format!("{}.{}", &significant[..1], &significant[1..])
        };
        return Ok(format!("{sign}{mantissa}e{exponent:+03}"));
    }
    let position = exponent + 1;
    if position <= 0 {
        Ok(format!(
            "{sign}0.{}{significant}",
            "0".repeat((-position) as usize)
        ))
    } else if position as usize >= significant.len() {
        Ok(format!(
            "{sign}{significant}{}.0",
            "0".repeat(position as usize - significant.len())
        ))
    } else {
        let (left, right) = significant.split_at(position as usize);
        Ok(format!("{sign}{left}.{right}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_dat;
    use fictrac_core::TrackRow;
    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    fn row() -> TrackRow {
        TrackRow {
            frame: 4,
            sequence: 7,
            delta_camera: [1.0, 2.0, 3.0],
            error: 4.0,
            delta_lab: [5.0, 6.0, 7.0],
            absolute_camera: [8.0, 9.0, 10.0],
            absolute_lab: [11.0, 12.0, 13.0],
            position: [0.1234567890123456, -0.7654321098765432],
            heading: 0.3000000000001234,
            direction: 17.0,
            speed: 18.0,
            integrated: [19.0, 20.0],
            timestamp_ms: 21.0,
            delta_ms: 24.0,
            accepted: true,
            status: "tracked",
            samples: 100,
        }
    }
    fn packet(row: &TrackRow) -> crate::Result<Vec<u8>> {
        let mut packet = b"FT, ".to_vec();
        write_dat(&mut packet, row)?;
        Ok(packet)
    }
    fn legacy(framing: ZmqFraming) -> ZmqPayloadSettings {
        ZmqPayloadSettings {
            mode: ZmqOutputMode::LegacyMatrexVr,
            framing,
            ..ZmqPayloadSettings::default()
        }
    }

    #[test]
    fn native_default_retains_the_complete_single_ft_frame() -> TestResult {
        let packet = packet(&row())?;
        let mut settings = ZmqPayloadSettings::default();
        assert_eq!(settings.mode, ZmqOutputMode::NativeFt);
        assert_eq!(settings.encode(packet.clone())?, vec![packet.clone()]);
        settings.framing = ZmqFraming::TopicJson;
        assert_eq!(settings.effective_framing(), ZmqFraming::Single);
        assert_eq!(settings.encode(packet.clone())?, vec![packet]);
        Ok(())
    }

    #[test]
    fn python_json_mapping_uses_ft_fields_with_their_transmitted_precision() -> TestResult {
        // Oracle from Python json.dumps + math.degrees on this FT row's comma
        // split fields, including FT at zero. Delta/absolute/integrated fields
        // deliberately differ so a shifted index or axis substitution fails.
        let expected = br#"{"x": 0.123456789012, "y": -0.765432109877, "z": 0.0, "yaw": 17.188733853924695, "pitch": 0.0, "roll": 0.0}"#;
        let frames = legacy(ZmqFraming::Single).encode(packet(&row())?)?;
        assert_eq!(frames, vec![expected.to_vec()]);
        let mut settings = legacy(ZmqFraming::TopicJson);
        settings.topic = "sphere-motion".into();
        let frames = settings.encode(packet(&row())?)?;
        assert_eq!(frames, vec![b"sphere-motion".to_vec(), expected.to_vec()]);
        Ok(())
    }

    #[test]
    fn supplied_fictrac_rows_match_the_original_python_republisher_bytes() -> TestResult {
        // Captured with json.dumps({x: float(fields[15]), y: float(fields[16]),
        // z: 0.0, yaw: math.degrees(float(fields[17])), pitch: 0.0, roll: 0.0})
        // from the attributed fixture. No Python runtime is used by these tests.
        let rows: Vec<_> = include_str!("../../../examples/benchmark/upstream.dat")
            .lines()
            .collect();
        for (index, expected) in [
            (
                0,
                r#"{"x": 0.0, "y": 0.0, "z": 0.0, "yaw": 0.0, "pitch": 0.0, "roll": 0.0}"#,
            ),
            (
                1,
                r#"{"x": 0.021835468906813, "y": -0.020168872552472, "z": 0.0, "yaw": 358.8493852611741, "pitch": 0.0, "roll": 0.0}"#,
            ),
            (
                119,
                r#"{"x": -2.1388493504324, "y": -1.8833665549713, "z": 0.0, "yaw": 139.48053969589395, "pitch": 0.0, "roll": 0.0}"#,
            ),
            (
                299,
                r#"{"x": 3.674266189723, "y": -2.6214661833162, "z": 0.0, "yaw": 354.3469074551094, "pitch": 0.0, "roll": 0.0}"#,
            ),
        ] {
            let row = rows.get(index).ok_or("missing reference FT row")?;
            let frames = legacy(ZmqFraming::Single).encode(format!("FT, {row}\n").into_bytes())?;
            assert_eq!(frames, vec![expected.as_bytes().to_vec()], "row {index}");
        }
        Ok(())
    }

    #[test]
    fn python_float_notation_preserves_signed_zero_and_scientific_boundaries() -> TestResult {
        for (value, expected) in [
            (-0.0, "-0.0"),
            (0.0, "0.0"),
            (1e-12, "1e-12"),
            (-1e-5, "-1e-05"),
            (0.0001, "0.0001"),
            (999999999999999.0, "999999999999999.0"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1e17, "1e+17"),
            (1e20, "1e+20"),
            (1e100, "1e+100"),
        ] {
            assert_eq!(python_float(value)?, expected);
        }
        let mut value = row();
        value.position = [-0.0, 0.00001];
        value.heading = -0.0;
        assert_eq!(
            legacy(ZmqFraming::Single).encode(packet(&value)?)?,
            vec![
                br#"{"x": -0.0, "y": 1e-05, "z": 0.0, "yaw": -0.0, "pitch": 0.0, "roll": 0.0}"#
                    .to_vec()
            ]
        );
        Ok(())
    }

    #[test]
    fn json_codec_is_stateless_across_cumulative_motion_and_epoch_resets() -> TestResult {
        let settings = legacy(ZmqFraming::Single);
        let mut value = row();
        value.position = [400.0, -500.0];
        value.heading = 3.0 * std::f64::consts::PI;
        let before = settings.encode(packet(&value)?)?;
        value.sequence = 0;
        value.delta_lab = [0.0; 3];
        value.integrated = [0.0; 2];
        assert_eq!(settings.encode(packet(&value)?)?, before);
        value.position = [0.0, 0.0];
        value.heading = 0.0;
        assert_eq!(
            settings.encode(packet(&value)?)?,
            vec![
                br#"{"x": 0.0, "y": 0.0, "z": 0.0, "yaw": 0.0, "pitch": 0.0, "roll": 0.0}"#
                    .to_vec()
            ]
        );
        Ok(())
    }

    #[test]
    fn malformed_nonfinite_and_degree_overflow_records_are_rejected() -> TestResult {
        let settings = legacy(ZmqFraming::Single);
        assert!(settings.encode(b"FT, 1, 2, 3".to_vec()).is_err());
        let bytes = packet(&row())?;
        assert!(settings.encode(bytes[4..].to_vec()).is_err());
        let mut value = row();
        for nonfinite in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            value.position[0] = nonfinite;
            assert!(settings.encode(packet(&value)?).is_err());
        }
        value = row();
        value.heading = f64::MAX;
        assert!(settings.encode(packet(&value)?).is_err());
        Ok(())
    }
}
