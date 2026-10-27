//! OS-native capture. The only FFI implementation is isolated in macos.rs.
use fictrac_core::GrayFrame;
mod controls;
#[cfg(any(target_os = "linux", test))]
mod identity;
#[cfg(any(not(target_os = "macos"), test))]
mod jpeg;
#[cfg(any(not(target_os = "macos"), test))]
mod packed;
#[cfg(any(not(target_os = "macos"), test))]
mod selection;
pub use controls::*;
#[cfg(any(target_os = "macos", test))]
use std::sync::{Arc, Condvar, Mutex};
#[derive(Debug, thiserror::Error)]
#[error("camera: {0}")]
pub struct Error(pub String);
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone)]
pub struct CameraInfo {
    pub id: u32,
    pub name: String,
}
#[derive(Debug)]
pub struct CameraFrame {
    pub gray: GrayFrame,
    pub rgb: Vec<u8>,
    pub sequence: u64,
}
#[derive(Debug, Default)]
#[cfg(any(target_os = "macos", test))]
pub(crate) struct Latest {
    state: Mutex<LatestState>,
    ready: Condvar,
}
#[derive(Debug, Default)]
#[cfg(any(target_os = "macos", test))]
struct LatestState {
    frame: Option<CameraFrame>,
    sequence: u64,
    expected_dimensions: Option<(usize, usize)>,
}
#[cfg(any(target_os = "macos", test))]
impl Latest {
    pub fn set_dimensions(&self, width: usize, height: usize) -> Result<()> {
        if !(2..=8192).contains(&width)
            || !(2..=8192).contains(&height)
            || width.checked_mul(height).is_none_or(|n| n > 8_000_000)
        {
            return Err(Error("invalid expected camera dimensions".into()));
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.expected_dimensions = Some((width, height));
        state.frame = None;
        Ok(())
    }
    pub fn publish(&self, mut frame: CameraFrame) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .expected_dimensions
            .is_some_and(|d| d != (frame.gray.width(), frame.gray.height()))
        {
            return;
        }
        state.sequence = state.sequence.saturating_add(1);
        frame.sequence = state.sequence;
        state.frame = Some(frame);
        self.ready.notify_one();
    }
    pub fn take(&self, timeout: std::time::Duration) -> Result<Option<CameraFrame>> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (mut state, _) = self
            .ready
            .wait_timeout_while(state, timeout, |s| s.frame.is_none())
            .map_err(|_| Error("capture queue poisoned".into()))?;
        Ok(state.frame.take())
    }
}
/// Validate the requested camera rate before touching an OS session. A nonpositive
/// finite value leaves rate selection to the native camera, matching legacy config.
#[cfg(any(target_os = "macos", test))]
fn requested_interval(fps: f64) -> Result<Option<(i64, i32)>> {
    if !fps.is_finite() || (fps > 0.0 && !(0.1..=1000.0).contains(&fps)) {
        return Err(Error(
            "requested camera fps must be finite and 0.1..1000, or nonpositive for automatic"
                .into(),
        ));
    }
    if fps <= 0.0 {
        return Ok(None);
    }
    let mut numerator = 1_000_000i64;
    let mut denominator = (fps * numerator as f64).round() as i64;
    let (mut a, mut b) = (numerator, denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    numerator /= a;
    denominator /= a;
    Ok(Some((numerator, denominator as i32)))
}
/// Clamp rational approximation to the exact supported native interval endpoints.
/// i128 products keep all CMTime i64/i32 comparisons exact, including 1/30 second.
#[cfg(any(target_os = "macos", test))]
fn supported_interval(
    fps: f64,
    requested: (i64, i32),
    shortest: (i64, i32),
    longest: (i64, i32),
) -> Option<(i64, i32)> {
    let valid = |t: (i64, i32)| t.0 > 0 && t.1 > 0;
    if !valid(requested) || !valid(shortest) || !valid(longest) {
        return None;
    }
    let compare = |a: (i64, i32), b: (i64, i32)| {
        (i128::from(a.0) * i128::from(b.1)).cmp(&(i128::from(b.0) * i128::from(a.1)))
    };
    if compare(shortest, longest).is_gt()
        || fps < longest.1 as f64 / longest.0 as f64
        || fps > shortest.1 as f64 / shortest.0 as f64
    {
        return None;
    }
    if compare(requested, shortest).is_le() {
        Some(shortest)
    } else if compare(requested, longest).is_ge() {
        Some(longest)
    } else {
        Some(requested)
    }
}
/// Convert padded BGRA safely; used by the native callback before releasing its pixel lock.
pub fn from_bgra(
    width: usize,
    height: usize,
    stride: usize,
    bytes: &[u8],
    timestamp_ms: f64,
) -> Result<CameraFrame> {
    if !(2..=8192).contains(&width) || !(2..=8192).contains(&height) {
        return Err(Error(
            "capture dimensions must be 2..8192 pixels per axis".into(),
        ));
    }
    if !timestamp_ms.is_finite() || timestamp_ms < 0.0 {
        return Err(Error(
            "capture timestamp must be finite and nonnegative".into(),
        ));
    }
    let size = width
        .checked_mul(height)
        .filter(|n| *n <= 8_000_000)
        .ok_or_else(|| Error("capture frame too large".into()))?;
    if stride < width.saturating_mul(4)
        || stride.checked_mul(height).is_none_or(|n| n > bytes.len())
    {
        return Err(Error("invalid BGRA row stride/buffer size".into()));
    }
    let mut rgb = Vec::with_capacity(size * 3);
    let mut gray = Vec::with_capacity(size);
    for y in 0..height {
        for x in 0..width {
            let i = y * stride + x * 4;
            let b = bytes[i];
            let g = bytes[i + 1];
            let r = bytes[i + 2];
            rgb.extend([r, g, b]);
            gray.push((0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0);
        }
    }
    Ok(CameraFrame {
        gray: GrayFrame::new(width, height, gray, timestamp_ms)
            .map_err(|e| Error(e.to_string()))?,
        rgb,
        sequence: 0,
    })
}
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{Camera, list_cameras, request_permission};
#[cfg(not(target_os = "macos"))]
mod clock;
#[cfg(not(target_os = "macos"))]
mod native;
#[cfg(not(target_os = "macos"))]
pub use native::{Camera, list_cameras, request_permission};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn padded_bgra_and_rejection() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let f = from_bgra(
            2,
            2,
            12,
            &[
                0, 0, 255, 0, 0, 255, 0, 0, 99, 99, 99, 99, 255, 0, 0, 0, 255, 255, 255, 0, 99, 99,
                99, 99,
            ],
            1.0,
        )?;
        assert_eq!(&f.rgb[..6], &[255, 0, 0, 0, 255, 0]);
        assert!((f.gray.pixels()[0] - 0.299).abs() < 1e-5);
        assert!(from_bgra(2, 2, 3, &[0; 24], 0.0).is_err());
        assert!(from_bgra(usize::MAX, 2, 8, &[], 0.0).is_err());
        // Reject dimensions before entering the row loop: zero width used to
        // allow an arbitrarily large height through the total-pixel check.
        assert!(from_bgra(0, usize::MAX, 0, &[], 0.0).is_err());
        assert!(from_bgra(8193, 2, 8193 * 4, &vec![0; 8193 * 8], 0.0).is_err());
        for timestamp in [f64::NAN, f64::INFINITY, -1.] {
            assert!(from_bgra(2, 2, 8, &[0; 16], timestamp).is_err());
        }
        Ok(())
    }
    #[test]
    fn latest_slot_is_bounded_and_keeps_capture_sequence()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let latest = Arc::new(Latest::default());
        for _ in 0..5 {
            latest.publish(from_bgra(2, 2, 8, &[0; 16], 0.0)?);
        }
        assert_eq!(
            latest
                .take(std::time::Duration::ZERO)?
                .ok_or("missing frame")?
                .sequence,
            5
        );
        Ok(())
    }
    #[test]
    fn camera_rate_intervals_are_exact_and_bound_checked()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        assert_eq!(requested_interval(30.0)?, Some((1, 30)));
        assert_eq!(requested_interval(29.97)?, Some((100, 2997)));
        assert_eq!(requested_interval(-1.0)?, None);
        assert_eq!(requested_interval(0.0)?, None);
        for rate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.01, 1000.01] {
            assert!(requested_interval(rate).is_err());
        }
        let ntsc = 30_000.0 / 1001.0;
        let approximated = requested_interval(ntsc)?.ok_or("interval missing")?;
        assert_eq!(
            supported_interval(ntsc, approximated, (1001, 30_000), (1, 15)),
            Some((1001, 30_000))
        );
        assert_eq!(
            supported_interval(30.0, (1, 30), (1, 30), (1, 15)),
            Some((1, 30))
        );
        assert_eq!(supported_interval(31.0, (1, 31), (1, 30), (1, 15)), None);
        assert_eq!(supported_interval(14.0, (1, 14), (1, 30), (1, 15)), None);
        assert_eq!(supported_interval(30.0, (1, 30), (0, 30), (1, 15)), None);
        Ok(())
    }
    #[test]
    fn startup_format_mismatch_is_filtered_without_advancing_sequence()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let latest = Latest::default();
        latest.set_dimensions(2, 2)?;
        latest.publish(from_bgra(3, 2, 12, &[0; 24], 0.0)?);
        assert!(latest.take(std::time::Duration::ZERO)?.is_none());
        latest.publish(from_bgra(2, 2, 8, &[0; 16], 1.0)?);
        assert_eq!(
            latest
                .take(std::time::Duration::ZERO)?
                .ok_or("matching frame missing")?
                .sequence,
            1
        );
        latest.publish(from_bgra(3, 2, 12, &[0; 24], 2.0)?);
        latest.publish(from_bgra(2, 2, 8, &[0; 16], 3.0)?);
        assert_eq!(
            latest
                .take(std::time::Duration::ZERO)?
                .ok_or("second matching frame missing")?
                .sequence,
            2
        );
        assert!(latest.set_dimensions(usize::MAX, 2).is_err());
        Ok(())
    }
}
