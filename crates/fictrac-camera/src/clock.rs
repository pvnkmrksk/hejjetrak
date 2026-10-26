//! Keep capture intervals independent of decoding, tracking and recording stalls.
use crate::{Error, Result};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct CaptureClock {
    origin: Option<(Duration, f64)>,
    previous: Option<f64>,
}

impl CaptureClock {
    pub fn timestamp(&mut self, captured: Option<Duration>, delivered_ms: f64) -> Result<f64> {
        if !delivered_ms.is_finite() || delivered_ms < 0.0 {
            return Err(Error(
                "camera delivery clock is invalid; reconnect the camera".into(),
            ));
        }
        let proposed_origin = self
            .origin
            .or_else(|| captured.map(|time| (time, delivered_ms)));
        let timestamp = if let (Some(time), Some((origin, offset))) = (captured, proposed_origin) {
            offset
                + time
                    .checked_sub(origin)
                    .ok_or_else(|| {
                        Error(
                            "camera capture timestamp moved backwards; reconnect the camera".into(),
                        )
                    })?
                    .as_secs_f64()
                    * 1000.0
        } else {
            delivered_ms
        };
        if !timestamp.is_finite() || self.previous.is_some_and(|previous| timestamp <= previous) {
            return Err(Error(
                "camera capture clock is discontinuous; reconnect the camera".into(),
            ));
        }
        self.origin = proposed_origin;
        self.previous = Some(timestamp);
        Ok(timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_frames_retain_capture_intervals_after_recording_stalls() -> Result<()> {
        let mut clock = CaptureClock::default();
        let origin = Duration::from_secs(1_700_000_000);
        assert_eq!(clock.timestamp(Some(origin), 400.0)?, 400.0);
        // Delivery was blocked for a quarter second; the next queued camera
        // image was exposed ten milliseconds after the previous image.
        assert_eq!(
            clock.timestamp(Some(origin + Duration::from_millis(10)), 650.0)?,
            410.0
        );
        assert_eq!(
            clock.timestamp(Some(origin + Duration::from_millis(20)), 651.0)?,
            420.0
        );
        // A real capture gap remains a gap instead of being compressed to the
        // time spent draining the queued images.
        assert_eq!(
            clock.timestamp(Some(origin + Duration::from_millis(60)), 652.0)?,
            460.0
        );
        Ok(())
    }

    #[test]
    fn invalid_capture_clock_does_not_advance_its_state() -> Result<()> {
        let mut clock = CaptureClock::default();
        let origin = Duration::from_secs(10);
        assert_eq!(clock.timestamp(Some(origin), 1.0)?, 1.0);
        assert!(clock.timestamp(Some(origin), 2.0).is_err());
        assert!(
            clock
                .timestamp(Some(origin - Duration::from_millis(1)), 3.0)
                .is_err()
        );
        assert_eq!(
            clock.timestamp(Some(origin + Duration::from_millis(10)), 4.0)?,
            11.0
        );
        Ok(())
    }

    #[test]
    fn backends_without_capture_timestamps_keep_host_elapsed_time() -> Result<()> {
        let mut clock = CaptureClock::default();
        assert_eq!(clock.timestamp(None, 1.0)?, 1.0);
        assert_eq!(clock.timestamp(None, 34.0)?, 34.0);
        assert!(clock.timestamp(None, f64::NAN).is_err());
        assert_eq!(clock.timestamp(None, 67.0)?, 67.0);
        Ok(())
    }
}
