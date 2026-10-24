use crate::{Error, Result};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct GrayFrame {
    width: usize,
    height: usize,
    // Validated pixels are immutable. Frontends, tracking and recording can
    // retain one observation without copying a full image on every handoff.
    pixels: Arc<Vec<f32>>,
    timestamp_ms: f64,
}
impl GrayFrame {
    /// Normalize byte pixels in one pass. The byte type proves the value range;
    /// unlike arbitrary f32 input it needs no second scan for NaNs/range errors.
    pub fn from_u8(width: usize, height: usize, pixels: &[u8], timestamp_ms: f64) -> Result<Self> {
        Self::from_u8_iter(width, height, pixels.iter().copied(), timestamp_ms)
    }
    pub fn from_u8_iter(
        width: usize,
        height: usize,
        pixels: impl ExactSizeIterator<Item = u8>,
        timestamp_ms: f64,
    ) -> Result<Self> {
        let size = width.checked_mul(height).filter(|n| *n <= 32_000_000);
        let count = pixels.len();
        if width < 2
            || height < 2
            || size != Some(count)
            || !timestamp_ms.is_finite()
            || timestamp_ms < 0.0
        {
            return Err(Error::Frame(
                "invalid dimensions, pixels or timestamp".into(),
            ));
        }
        // Even an iterator with a false size hint cannot allocate without bound
        // or create an invalid frame. This count check precedes publication.
        let values: Vec<_> = pixels
            .take(count + 1)
            .map(|p| f32::from(p) / 255.0)
            .collect();
        if values.len() != count {
            return Err(Error::Frame(
                "byte iterator length differs from frame dimensions".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            pixels: Arc::new(values),
            timestamp_ms,
        })
    }
    pub fn new(width: usize, height: usize, pixels: Vec<f32>, timestamp_ms: f64) -> Result<Self> {
        let size = width.checked_mul(height).filter(|n| *n <= 32_000_000);
        if width < 2
            || height < 2
            || size != Some(pixels.len())
            || !timestamp_ms.is_finite()
            || timestamp_ms < 0.0
            || pixels
                .iter()
                .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
        {
            return Err(Error::Frame(
                "invalid dimensions, pixels or timestamp".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            pixels: Arc::new(pixels),
            timestamp_ms,
        })
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn pixels(&self) -> &[f32] {
        &self.pixels
    }
    pub fn timestamp_ms(&self) -> f64 {
        self.timestamp_ms
    }
    /// Change only the clock, retaining the already validated immutable pixels.
    pub fn with_timestamp(mut self, timestamp_ms: f64) -> Result<Self> {
        if !timestamp_ms.is_finite() || timestamp_ms < 0.0 {
            return Err(Error::Frame(
                "invalid timestamp: expected a finite, nonnegative number".into(),
            ));
        }
        self.timestamp_ms = timestamp_ms;
        Ok(self)
    }
    pub fn sample(&self, x: f64, y: f64) -> Option<f64> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x >= (self.width - 1) as f64
            || y >= (self.height - 1) as f64
        {
            return None;
        }
        let ix = x as usize;
        let iy = y as usize;
        let fx = x - ix as f64;
        let fy = y - iy as f64;
        let i = iy.checked_mul(self.width)?.checked_add(ix)?;
        let a = f64::from(*self.pixels.get(i)?);
        let b = f64::from(*self.pixels.get(i + 1)?);
        let c = f64::from(*self.pixels.get(i + self.width)?);
        let d = f64::from(*self.pixels.get(i + self.width + 1)?);
        Some((a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy)
    }
    pub(crate) fn half(&self) -> Self {
        let width = self.width / 2;
        let height = self.height / 2;
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let i = 2 * y * self.width + 2 * x;
                // Indices are bounded by this validated frame's dimensions.
                pixels.push(
                    (self.pixels[i]
                        + self.pixels[i + 1]
                        + self.pixels[i + self.width]
                        + self.pixels[i + self.width + 1])
                        * 0.25,
                );
            }
        }
        Self {
            width,
            height,
            pixels: Arc::new(pixels),
            timestamp_ms: self.timestamp_ms,
        }
    }
}
