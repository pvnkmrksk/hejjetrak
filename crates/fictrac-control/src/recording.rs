//! Shared headless media recording, using Rust codecs and actual numerical views.
use crate::{Result, display};
use fictrac_core::{Config, DebugViews, GrayFrame, TrackRow};
use fictrac_io::{ConfigDocument, RecordingCodec, VideoRecorder};
use std::path::Path;

pub struct MediaRecorders {
    raw: Option<VideoRecorder>,
    debug: Option<VideoRecorder>,
    config: Config,
    poses: Vec<[f64; 3]>,
}
impl MediaRecorders {
    pub fn create(
        directory: &Path,
        doc: &ConfigDocument,
        config: &Config,
        fps: f64,
    ) -> Result<Self> {
        let codec = if doc.boolean("save_raw") || doc.boolean("save_debug") {
            RecordingCodec::from_legacy(doc.get("vid_codec")).map_err(display)?
        } else {
            RecordingCodec::Raw
        };
        let raw = if doc.boolean("save_raw") {
            Some(
                VideoRecorder::create(
                    &directory.join(format!("raw.{}", codec.extension())),
                    config.width,
                    config.height,
                    fps,
                    codec,
                )
                .map_err(display)?,
            )
        } else {
            None
        };
        let debug = if doc.boolean("save_debug") {
            Some(
                VideoRecorder::create(
                    &directory.join(format!("debug.{}", codec.extension())),
                    640,
                    480,
                    fps,
                    codec,
                )
                .map_err(display)?,
            )
        } else {
            None
        };
        Ok(Self {
            raw,
            debug,
            config: config.clone(),
            poses: vec![],
        })
    }
    pub fn active(&self) -> bool {
        self.raw.is_some() || self.debug.is_some()
    }
    pub fn write(
        &mut self,
        rgb: &[u8],
        width: usize,
        height: usize,
        views: &DebugViews,
        path: &[[f64; 2]],
    ) -> Result<()> {
        if let Some(raw) = &mut self.raw {
            raw.write_rgb(rgb).map_err(display)?;
        }
        if let Some(debug) = &mut self.debug {
            if width != self.config.width || height != self.config.height {
                return Err("recording frame dimensions changed".into());
            }
            let image = fictrac_io::render_calibrated_debug_rgb(
                rgb,
                &self.config,
                views,
                path,
                &self.poses,
                160,
            )
            .map_err(display)?;
            debug.write_rgb(&image).map_err(display)?;
        }
        Ok(())
    }
    pub fn write_row(
        &mut self,
        rgb: &[u8],
        frame: &GrayFrame,
        views: &DebugViews,
        path: &[[f64; 2]],
        row: &TrackRow,
    ) -> Result<()> {
        if row.accepted {
            if self.poses.len() >= 1000 {
                self.poses.remove(0);
            }
            self.poses.push(row.absolute_camera);
        }
        self.write(rgb, frame.width(), frame.height(), views, path)
    }
    pub fn finish(self) -> Result<()> {
        if let Some(raw) = self.raw {
            raw.finish().map_err(display)?;
        }
        if let Some(debug) = self.debug {
            debug.finish().map_err(display)?;
        }
        Ok(())
    }
}
pub fn gray_rgb(frame: &GrayFrame) -> Vec<u8> {
    frame
        .pixels()
        .iter()
        .flat_map(|p| {
            let v = (p * 255.).round() as u8;
            [v, v, v]
        })
        .collect()
}
pub(crate) struct Recording {
    dat: std::fs::File,
    all: std::fs::File,
    quality: std::fs::File,
    media: MediaRecorders,
    path: Vec<[f64; 2]>,
}
impl Recording {
    pub(crate) fn create(
        directory: &Path,
        doc: &ConfigDocument,
        config: &Config,
        fps: f64,
    ) -> Result<Self> {
        use std::io::Write;
        crate::create_directory(directory)?;
        let dat = crate::create(&directory.join("tracking.dat"))?;
        let all = crate::create(&directory.join("all-frames.dat"))?;
        let mut quality = crate::create(&directory.join("quality.csv"))?;
        writeln!(
            quality,
            "frame,timestamp_ms,accepted,status,fit_error,samples"
        )
        .map_err(display)?;
        let mut file = crate::create(&directory.join("config.txt"))?;
        file.write_all(doc.text().as_bytes()).map_err(display)?;
        let media = MediaRecorders::create(directory, doc, config, fps)?;
        Ok(Self {
            dat,
            all,
            quality,
            media,
            path: vec![],
        })
    }
    pub(crate) fn write(
        &mut self,
        row: &TrackRow,
        frame: &GrayFrame,
        rgb: &[u8],
        views: &DebugViews,
        legacy: bool,
    ) -> Result<()> {
        use std::io::Write;
        if row.accepted || !legacy {
            fictrac_io::write_dat(&mut self.dat, row).map_err(display)?;
        }
        fictrac_io::write_dat(&mut self.all, row).map_err(display)?;
        writeln!(
            self.quality,
            "{},{:.9},{},{},{:.12},{}",
            row.frame, row.timestamp_ms, row.accepted, row.status, row.error, row.samples
        )
        .map_err(display)?;
        if self.path.len() >= 100_000 {
            self.path = self.path.iter().step_by(2).copied().collect();
        }
        self.path.push(row.position);
        self.media.write_row(rgb, frame, views, &self.path, row)
    }
    pub(crate) fn finish(self) -> Result<()> {
        self.dat.sync_all().map_err(display)?;
        self.all.sync_all().map_err(display)?;
        self.quality.sync_all().map_err(display)?;
        self.media.finish()
    }
}
