//! Bounded, ordered decode-ahead. Numerical tracking stays on its owning thread.
use crate::{Error, Result, VideoInfo, VideoSource, rgb_tracking_frame};
use fictrac_core::GrayFrame;
use std::{
    collections::VecDeque,
    sync::mpsc::{Receiver, sync_channel},
    thread::{self, JoinHandle},
};

struct Decoded {
    gray: GrayFrame,
    rgb: Option<Vec<u8>>,
}

/// A one-slot decoder pipeline. No frames are skipped, and backpressure blocks
/// the producer instead of growing memory. There can also be one frame being
/// decoded, plus two frames retained temporarily for recording-rate inference.
/// Enable color on `VideoSource` before transferring it with `prefetch()`.
pub struct PrefetchedVideoSource {
    pub info: VideoInfo,
    color: bool,
    last_rgb: Option<Vec<u8>>,
    pending: VecDeque<Decoded>,
    receiver: Option<Receiver<Result<Option<Decoded>>>>,
    worker: Option<JoinHandle<()>>,
    ended: bool,
    failure: Option<String>,
}

impl PrefetchedVideoSource {
    pub(crate) fn new(mut source: VideoSource, color: bool) -> Result<Self> {
        let info = source.info.clone();
        let last_rgb = source.take_rgb();
        let (sender, receiver) = sync_channel(1);
        let worker = thread::Builder::new()
            .name("hejjetrak-decode".into())
            .spawn(move || {
                loop {
                    let next = source.next_frame().map(|frame| {
                        frame.map(|gray| Decoded {
                            gray,
                            rgb: source.take_rgb(),
                        })
                    });
                    let terminal = !matches!(&next, Ok(Some(_)));
                    if sender.send(next).is_err() || terminal {
                        break;
                    }
                }
            })
            .map_err(|e| Error::Video(format!("cannot start decoder worker: {e}")))?;
        Ok(Self {
            info,
            color,
            last_rgb,
            pending: VecDeque::new(),
            receiver: Some(receiver),
            worker: Some(worker),
            ended: false,
            failure: None,
        })
    }

    fn receive(&mut self) -> Result<Option<Decoded>> {
        if let Some(failure) = &self.failure {
            return Err(Error::Video(format!(
                "decoder stopped after error: {failure}"
            )));
        }
        if self.ended {
            return Ok(None);
        }
        let response = self
            .receiver
            .as_ref()
            .ok_or_else(|| Error::Video("decoder pipeline is closed".into()))?
            .recv();
        match response {
            Ok(Ok(Some(frame))) => Ok(Some(frame)),
            Ok(Ok(None)) => {
                self.ended = true;
                Ok(None)
            }
            Ok(Err(error)) => {
                self.failure = Some(error.to_string());
                Err(error)
            }
            Err(_) => {
                let message =
                    "decoder worker stopped before EOF; input may be corrupt or unsupported";
                self.failure = Some(message.into());
                Err(Error::Video(message.into()))
            }
        }
    }

    pub fn next_frame(&mut self) -> Result<Option<GrayFrame>> {
        let next = if let Some(frame) = self.pending.pop_front() {
            Some(frame)
        } else {
            self.receive()?
        };
        self.last_rgb = None;
        Ok(next.map(|frame| {
            self.last_rgb = frame.rgb;
            frame.gray
        }))
    }

    pub fn take_rgb(&mut self) -> Option<Vec<u8>> {
        self.last_rgb.take()
    }

    pub fn next_tracking_frame(&mut self, channel: Option<usize>) -> Result<Option<GrayFrame>> {
        if !self.color {
            return Err(Error::Video(
                "enable color before prefetch() for RGB-compatible tracking".into(),
            ));
        }
        let Some(frame) = self.next_frame()? else {
            return Ok(None);
        };
        match self.last_rgb.as_deref() {
            Some(rgb) => rgb_tracking_frame(frame, rgb, channel).map(Some),
            None => Ok(Some(frame)),
        }
    }

    /// Inspect the next two PTS values without consuming their pixels or RGB.
    /// This preserves the synchronous adapter's source-clock semantics.
    pub fn recording_fps(&mut self) -> Result<f64> {
        if let Some(fps) = self.info.fps {
            if fps.is_finite() && (0.1..=1000.0).contains(&fps) {
                return Ok(fps);
            }
            return Err(Error::Video("source recording fps is outside supported 0.1..1000; provide an explicit valid src_fps".into()));
        }
        while self.pending.len() < 2 {
            let Some(frame) = self.receive()? else {
                break;
            };
            self.pending.push_back(frame);
        }
        if self.pending.len() < 2 {
            return Err(Error::Video("recording fps unavailable: source has fewer than two remaining timed frames; set src_fps explicitly".into()));
        }
        let delta = self.pending[1].gray.timestamp_ms() - self.pending[0].gray.timestamp_ms();
        let fps = 1000.0 / delta;
        if !fps.is_finite() || !(0.1..=1000.0).contains(&fps) {
            return Err(Error::Video("cannot infer a supported recording fps from source timestamps; set src_fps explicitly".into()));
        }
        self.info.fps = Some(fps);
        Ok(fps)
    }
}

impl Drop for PrefetchedVideoSource {
    fn drop(&mut self) {
        // Disconnect before joining: a paused consumer may leave the one-slot
        // queue full and its producer blocked in send(). Dropping the receiver
        // wakes that send, without polling or detaching the decoding thread.
        self.receiver.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
