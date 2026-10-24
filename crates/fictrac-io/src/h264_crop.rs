//! Progressive Baseline/Main/High 8-bit 4:2:0 padding and normative SPS cropping.
//! Encoded macroblocks cover an edge-extended image; the SPS advertises the
//! original even display dimensions. Container metadata must use those dimensions.
use crate::{Error, Result};
use oxideav_h264::{
    avcc::AvcDecoderConfigurationRecord, encoder::nal::insert_emulation_prevention,
    nal::parse_nal_unit, sps::Sps,
};

const MAX_PACKET: usize = 64_000_000;
const MAX_SPS: usize = 65_535;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct H264Layout {
    pub width: usize,
    pub height: usize,
    pub coded_width: usize,
    pub coded_height: usize,
}
fn error(message: impl std::fmt::Display) -> Error {
    Error::Video(format!("H.264 cropping: {message}"))
}
impl H264Layout {
    pub fn new(width: usize, height: usize) -> Result<Self> {
        if !(16..=8192).contains(&width)
            || !(16..=8192).contains(&height)
            || !width.is_multiple_of(2)
            || !height.is_multiple_of(2)
        {
            return Err(error(
                "display dimensions must be even, 16..8192 pixels, for 4:2:0 recording",
            ));
        }
        let coded_width = width
            .checked_add(15)
            .map(|v| v / 16 * 16)
            .ok_or_else(|| error("coded width overflow"))?;
        let coded_height = height
            .checked_add(15)
            .map(|v| v / 16 * 16)
            .ok_or_else(|| error("coded height overflow"))?;
        if coded_width
            .checked_mul(coded_height)
            .is_none_or(|n| n > 32_000_000)
        {
            return Err(error("padded frame exceeds 32 million pixels"));
        }
        Ok(Self {
            width,
            height,
            coded_width,
            coded_height,
        })
    }
    /// Extend the right/bottom edge, preserving every source RGB triplet exactly.
    pub fn pad_rgb(&self, rgb: &[u8]) -> Result<Vec<u8>> {
        self.validate()?;
        if self
            .width
            .checked_mul(self.height)
            .and_then(|n| n.checked_mul(3))
            != Some(rgb.len())
        {
            return Err(error("RGB length does not match display dimensions"));
        }
        let mut out = vec![0; self.coded_width * self.coded_height * 3];
        for y in 0..self.coded_height {
            let source = y.min(self.height - 1) * self.width * 3;
            let target = y * self.coded_width * 3;
            out[target..target + self.width * 3]
                .copy_from_slice(&rgb[source..source + self.width * 3]);
            for x in self.width..self.coded_width {
                let pixel = source + (self.width - 1) * 3;
                out[target + x * 3..target + x * 3 + 3].copy_from_slice(&rgb[pixel..pixel + 3]);
            }
        }
        Ok(out)
    }
    fn validate(&self) -> Result<()> {
        if Self::new(self.width, self.height)? != *self {
            Err(error("coded dimensions do not match the display layout"))
        } else {
            Ok(())
        }
    }
    /// Patch only SPS NALs; other NALs, prefix lengths and byte-stream zeros stay exact.
    pub fn patch_annex_b(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        self.validate()?;
        if bytes.len() > MAX_PACKET {
            return Err(error("encoded packet exceeds 64 MB"));
        }
        let mut prefixes = vec![];
        let mut cursor = 0;
        while let Some((position, length)) = prefix(bytes, cursor) {
            if prefixes.len() >= 4096 {
                return Err(error("encoded packet has too many NAL units"));
            }
            prefixes.push((position, length));
            cursor = position + length;
        }
        if prefixes.is_empty() {
            return Err(error("expected Annex B start codes"));
        }
        if bytes[..prefixes[0].0].iter().any(|b| *b != 0) {
            return Err(error("nonzero bytes precede the first Annex B NAL"));
        }
        let mut output = Vec::with_capacity(bytes.len() + 64);
        output.extend_from_slice(&bytes[..prefixes[0].0]);
        for (index, (start, length)) in prefixes.iter().copied().enumerate() {
            let end = prefixes.get(index + 1).map(|v| v.0).unwrap_or(bytes.len());
            let payload = start + length;
            if payload >= end {
                return Err(error("empty Annex B NAL unit"));
            }
            output.extend_from_slice(&bytes[start..payload]);
            if bytes[payload] & 31 == 7 {
                let mut nal_end = end;
                while nal_end > payload && bytes[nal_end - 1] == 0 {
                    nal_end -= 1;
                }
                output.extend_from_slice(&self.patch_sps(&bytes[payload..nal_end])?);
                output.extend_from_slice(&bytes[nal_end..end]);
            } else {
                output.extend_from_slice(&bytes[payload..end]);
            }
        }
        Ok(output)
    }
    /// Patch all SPS entries of an AVCDecoderConfigurationRecord, preserving PPS/VUI.
    pub fn patch_avcc(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        self.validate()?;
        if bytes.len() > MAX_SPS {
            return Err(error("AVC configuration exceeds 65535 bytes"));
        }
        let mut record = AvcDecoderConfigurationRecord::parse(bytes).map_err(error)?;
        let mut changed = false;
        for sps in &mut record.sps {
            let patched = self.patch_sps(sps)?;
            changed |= patched != *sps;
            *sps = patched;
        }
        if changed {
            record.to_bytes().map_err(error)
        } else {
            Ok(bytes.to_vec())
        }
    }
    fn patch_sps(&self, nal: &[u8]) -> Result<Vec<u8>> {
        if nal.len() > MAX_SPS || nal.len() < 2 || nal[0] & 31 != 7 {
            return Err(error("invalid SPS NAL length/type"));
        }
        let parsed = parse_nal_unit(nal).map_err(error)?;
        let rbsp = parsed.rbsp.as_ref();
        let sps = Sps::parse(rbsp).map_err(error)?;
        let (crop_start, crop_end, body_end) = crop_range(rbsp)?;
        if !matches!(sps.profile_idc, 66 | 77 | 100)
            || !sps.frame_mbs_only_flag
            || sps.chroma_format_idc != 1
            || sps.bit_depth_luma_minus8 != 0
            || sps.bit_depth_chroma_minus8 != 0
        {
            return Err(error(
                "cropping supports progressive Baseline/Main/High 8-bit 4:2:0 streams",
            ));
        }
        if (sps.pic_width_in_mbs() as usize).checked_mul(16) != Some(self.coded_width)
            || (sps.frame_height_in_mbs() as usize).checked_mul(16) != Some(self.coded_height)
        {
            return Err(error("SPS coded dimensions differ from the padded frame"));
        }
        if sps.cropped_dimensions() == (self.width as u32, self.height as u32) {
            return Ok(nal.to_vec());
        }
        if crop_end > body_end {
            return Err(error("SPS crop fields extend beyond its body"));
        }
        let mut bits = Vec::with_capacity(body_end + 64);
        copy_bits(&mut bits, rbsp, 0, crop_start);
        bits.push(true);
        for offset in [
            0,
            (self.coded_width - self.width) / 2,
            0,
            (self.coded_height - self.height) / 2,
        ] {
            write_ue(&mut bits, offset as u32);
        }
        copy_bits(&mut bits, rbsp, crop_end, body_end);
        bits.push(true); // rbsp_stop_one_bit, followed by byte alignment zeros.
        while !bits.len().is_multiple_of(8) {
            bits.push(false);
        }
        let mut patched = vec![0; bits.len() / 8];
        for (index, bit) in bits.into_iter().enumerate() {
            if bit {
                patched[index / 8] |= 1 << (7 - index % 8);
            }
        }
        let mut out = vec![nal[0]];
        out.extend_from_slice(&insert_emulation_prevention(&patched));
        Ok(out)
    }
}
fn prefix(bytes: &[u8], mut start: usize) -> Option<(usize, usize)> {
    while start + 3 <= bytes.len() {
        if bytes.get(start..start + 4) == Some(&[0, 0, 0, 1]) {
            return Some((start, 4));
        }
        if bytes[start..start + 3] == [0, 0, 1] {
            return Some((start, 3));
        }
        start += 1;
    }
    None
}
fn position(reader: &Bits<'_>) -> usize {
    reader.position
}
/// Locate the crop flag without interpreting or rebuilding the subsequent VUI.
/// Every read is bounded; Exp-Golomb leading-zero runs are capped to 31 bits.
fn crop_range(rbsp: &[u8]) -> Result<(usize, usize, usize)> {
    let mut r = Bits::new(rbsp);
    let profile = r.u(8).map_err(error)?;
    if !matches!(profile, 66 | 77 | 100) {
        return Err(error("unsupported SPS profile"));
    }
    r.u(16).map_err(error)?; // constraints and level.
    r.ue().map_err(error)?; // seq_parameter_set_id.
    if profile == 100 {
        if r.ue()? != 1 {
            return Err(error(
                "High SPS cropping requires 4:2:0 chroma_format_idc=1",
            ));
        }
        let luma_depth = r.ue()?;
        let chroma_depth = r.ue()?;
        if luma_depth != 0 || chroma_depth != 0 {
            return Err(error("High SPS cropping requires 8-bit luma and chroma"));
        }
        r.u(1)?; // qpprime_y_zero_transform_bypass_flag does not change cropping units.
        if r.u(1)? != 0 {
            // High 4:2:0 has six 4x4 and two 8x8 sequence scaling-list slots.
            for index in 0..8 {
                if r.u(1)? != 0 {
                    skip_scaling_list(&mut r, if index < 6 { 16 } else { 64 })?;
                }
            }
        }
    }
    r.ue().map_err(error)?; // log2_max_frame_num_minus4.
    match r.ue().map_err(error)? {
        0 => {
            r.ue().map_err(error)?;
        }
        1 => {
            r.u(1).map_err(error)?;
            r.se().map_err(error)?;
            r.se().map_err(error)?;
            let cycle = r.ue().map_err(error)?;
            if cycle > 255 {
                return Err(error("picture-order cycle exceeds 255"));
            }
            for _ in 0..cycle {
                r.se().map_err(error)?;
            }
        }
        2 => {}
        _ => return Err(error("invalid picture-order count type")),
    }
    r.ue().map_err(error)?;
    r.u(1).map_err(error)?;
    r.ue().map_err(error)?;
    r.ue().map_err(error)?;
    if r.u(1).map_err(error)? != 1 {
        return Err(error("interlaced SPS cropping is unsupported"));
    }
    r.u(1).map_err(error)?; // direct_8x8_inference_flag.
    let start = position(&r);
    if r.u(1).map_err(error)? != 0 {
        for _ in 0..4 {
            r.ue().map_err(error)?;
        }
    }
    let end = position(&r);
    if r.u(1)? != 0 {
        skip_vui(&mut r)?;
    }
    let body_end = position(&r);
    if r.u(1)? != 1 {
        return Err(error("SPS has no RBSP stop bit"));
    }
    while !position(&r).is_multiple_of(8) {
        if r.u(1)? != 0 {
            return Err(error("SPS trailing alignment bits must be zero"));
        }
    }
    if position(&r) != rbsp.len() * 8 {
        return Err(error("unexpected bytes after SPS trailing bits"));
    }
    Ok((start, end, body_end))
}
struct Bits<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Bits<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn u(&mut self, width: usize) -> Result<u32> {
        if width > 32
            || self
                .position
                .checked_add(width)
                .is_none_or(|end| end > self.bytes.len() * 8)
        {
            return Err(error("truncated or oversized bit field"));
        }
        let mut value = 0;
        for _ in 0..width {
            value = (value << 1)
                | ((self.bytes[self.position / 8] >> (7 - self.position % 8)) & 1) as u32;
            self.position += 1;
        }
        Ok(value)
    }
    fn ue(&mut self) -> Result<u32> {
        let mut zeros = 0usize;
        while self.u(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return Err(error("Exp-Golomb value exceeds 32-bit range"));
            }
        }
        let suffix = self.u(zeros)?;
        Ok(((1u64 << zeros) - 1 + suffix as u64) as u32)
    }
    fn se(&mut self) -> Result<i64> {
        let value = self.ue()? as i64;
        Ok(if value % 2 == 0 {
            -value / 2
        } else {
            (value + 1) / 2
        })
    }
}
fn skip_vui(r: &mut Bits<'_>) -> Result<()> {
    if r.u(1)? != 0 && r.u(8)? == 255 {
        r.u(16)?;
        r.u(16)?;
    }
    if r.u(1)? != 0 {
        r.u(1)?;
    }
    if r.u(1)? != 0 {
        r.u(3)?;
        r.u(1)?;
        if r.u(1)? != 0 {
            r.u(8)?;
            r.u(8)?;
            r.u(8)?;
        }
    }
    if r.u(1)? != 0 {
        r.ue()?;
        r.ue()?;
    }
    if r.u(1)? != 0 {
        r.u(32)?;
        r.u(32)?;
        r.u(1)?;
    }
    let nal_hrd = r.u(1)? != 0;
    if nal_hrd {
        skip_hrd(r)?;
    }
    let vcl_hrd = r.u(1)? != 0;
    if vcl_hrd {
        skip_hrd(r)?;
    }
    if nal_hrd || vcl_hrd {
        r.u(1)?;
    }
    r.u(1)?;
    if r.u(1)? != 0 {
        r.u(1)?;
        for _ in 0..6 {
            r.ue()?;
        }
    }
    Ok(())
}
fn skip_hrd(r: &mut Bits<'_>) -> Result<()> {
    let count = r.ue()?;
    if count > 31 {
        return Err(error("HRD CPB count exceeds 32 entries"));
    }
    r.u(4)?;
    r.u(4)?;
    for _ in 0..=count {
        r.ue()?;
        r.ue()?;
        r.u(1)?;
    }
    for _ in 0..4 {
        r.u(5)?;
    }
    Ok(())
}
fn skip_scaling_list(r: &mut Bits<'_>, size: usize) -> Result<()> {
    let mut last = 8i64;
    let mut next = 8i64;
    for _ in 0..size {
        if next != 0 {
            let delta = r.se()?;
            if !(-128..=127).contains(&delta) {
                return Err(error("SPS scaling-list delta_scale is outside -128..127"));
            }
            next = (last + delta + 256) % 256;
        }
        if next != 0 {
            last = next;
        }
    }
    Ok(())
}
fn copy_bits(output: &mut Vec<bool>, bytes: &[u8], start: usize, end: usize) {
    for index in start..end {
        output.push(bytes[index / 8] & (1 << (7 - index % 8)) != 0);
    }
}
fn write_ue(output: &mut Vec<bool>, value: u32) {
    let code = value as u64 + 1;
    let width = 64 - code.leading_zeros() as usize;
    output.extend(std::iter::repeat_n(false, width - 1));
    for shift in (0..width).rev() {
        output.push(code & (1 << shift) != 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideav_h264::{
        encoder::{
            ScalingListsSpec,
            nal::build_nal_unit,
            sps::{BaselineSpsConfig, build_baseline_sps_rbsp, build_sps_rbsp_with_bypass},
        },
        nal::{AnnexBSplitter, NalUnitType},
        vui::{TimingInfo, VuiParameters},
    };
    #[test]
    fn padding_extends_edges_and_rejects_odd_or_forged_layouts() -> Result<()> {
        let layout = H264Layout::new(34, 18)?;
        assert_eq!((layout.coded_width, layout.coded_height), (48, 32));
        let rgb = (0..34 * 18 * 3)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>();
        let padded = layout.pad_rgb(&rgb)?;
        for y in 0..32 {
            for x in 0..48 {
                let source = (y.min(17) * 34 + x.min(33)) * 3;
                let target = (y * 48 + x) * 3;
                assert_eq!(&padded[target..target + 3], &rgb[source..source + 3]);
            }
        }
        assert!(H264Layout::new(33, 18).is_err());
        assert!(H264Layout::new(34, 17).is_err());
        assert!(H264Layout::new(usize::MAX, 18).is_err());
        let mut forged = layout;
        forged.coded_width = 16;
        assert!(forged.pad_rgb(&rgb).is_err());
        Ok(())
    }
    #[test]
    fn cropping_preserves_main_vui_annex_b_and_avcc_and_is_idempotent() -> Result<()> {
        let layout = H264Layout::new(34, 18)?;
        let config = BaselineSpsConfig {
            width_in_mbs: 3,
            height_in_mbs: 2,
            profile_idc: 77,
            vui: Some(VuiParameters {
                timing_info: Some(TimingInfo {
                    num_units_in_tick: 1001,
                    time_scale: 60000,
                    fixed_frame_rate_flag: true,
                }),
                ..VuiParameters::default()
            }),
            ..BaselineSpsConfig::default()
        };
        let rbsp = build_baseline_sps_rbsp(&config);
        let nal = build_nal_unit(3, NalUnitType::Sps, &rbsp);
        let patched = layout.patch_annex_b(&nal)?;
        let raw = AnnexBSplitter::new(&patched)
            .next()
            .ok_or_else(|| error("no SPS"))?;
        let sps = Sps::parse(&parse_nal_unit(raw).map_err(error)?.rbsp).map_err(error)?;
        assert_eq!(sps.cropped_dimensions(), (34, 18));
        assert_eq!(sps.vui, Sps::parse(&rbsp).map_err(error)?.vui);
        assert_eq!(layout.patch_annex_b(&patched)?, patched);
        let original = AnnexBSplitter::new(&nal)
            .next()
            .ok_or_else(|| error("no original SPS"))?;
        let record = AvcDecoderConfigurationRecord::from_parameter_sets(&[original], &[], &[])
            .map_err(error)?
            .to_bytes()
            .map_err(error)?;
        let patched = layout.patch_avcc(&record)?;
        assert_eq!(
            AvcDecoderConfigurationRecord::parse(&patched)
                .map_err(error)?
                .parsed_sps()
                .map_err(error)?
                .cropped_dimensions(),
            (34, 18)
        );
        assert_eq!(layout.patch_avcc(&patched)?, patched);
        let aligned = H264Layout::new(48, 32)?;
        assert_eq!(aligned.patch_annex_b(&nal)?, nal);
        assert_eq!(aligned.patch_avcc(&record)?, record);
        assert!(layout.patch_annex_b(&[0, 0, 1, 0x67, 0]).is_err());
        assert!(layout.patch_avcc(&[1, 77]).is_err());
        Ok(())
    }
    #[test]
    fn actual_encoder_padded_cropped_avi_and_mp4_decode_rgb_and_timestamps() -> Result<()> {
        use crate::{RecordingCodec, VideoRecorder, VideoSource};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "fictrac-h264-crop-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory)?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        for codec in [RecordingCodec::H264, RecordingCodec::H264Mp4] {
            let filename = directory.join(format!("cropped.{}", codec.extension()));
            let mut recorder = VideoRecorder::create(&filename, 34, 18, 30., codec)?;
            for index in 0..3 {
                let pixel = [50 + index * 15, 100, 150];
                let rgb = (0..34 * 18).flat_map(|_| pixel).collect::<Vec<_>>();
                recorder.write_rgb(&rgb)?;
            }
            recorder.finish()?;
            let mut video = VideoSource::open(&filename)?;
            video.enable_color();
            assert_eq!((video.info.width, video.info.height), (34, 18));
            let mut count = 0;
            while let Some(frame) = video.next_frame()? {
                assert_eq!((frame.width(), frame.height()), (34, 18));
                assert!((frame.timestamp_ms() - count as f64 * 1000. / 30.).abs() < 1e-6);
                let rgb = video
                    .take_rgb()
                    .ok_or_else(|| error("decoded RGB missing"))?;
                assert_eq!(rgb.len(), 34 * 18 * 3);
                let expected = [50 + count * 15, 100, 150];
                for pixel in rgb.as_chunks::<3>().0 {
                    for (actual, wanted) in pixel.iter().zip(expected) {
                        assert!(
                            (*actual as i32 - wanted).abs() <= 12,
                            "decoded channel {actual} expected {wanted}"
                        );
                    }
                }
                count += 1;
            }
            assert_eq!(count, 3);
        }
        assert_eq!(H264Layout::new(640, 360)?.coded_height, 368);
        assert_eq!(H264Layout::new(1920, 1080)?.coded_height, 1088);
        Ok(())
    }
    #[test]
    fn high_8bit_420_cropping_preserves_scaling_bypass_and_vui() -> Result<()> {
        let layout = H264Layout::new(34, 18)?;
        for scaling in [false, true] {
            let config = BaselineSpsConfig {
                width_in_mbs: 3,
                height_in_mbs: 2,
                profile_idc: 100,
                seq_scaling_lists: scaling.then_some(ScalingListsSpec::Default),
                vui: Some(VuiParameters {
                    timing_info: Some(TimingInfo {
                        num_units_in_tick: 1001,
                        time_scale: 60000,
                        fixed_frame_rate_flag: true,
                    }),
                    ..VuiParameters::default()
                }),
                ..BaselineSpsConfig::default()
            };
            let rbsp = build_sps_rbsp_with_bypass(&config, true);
            let nal = build_nal_unit(3, NalUnitType::Sps, &rbsp);
            let patched = layout.patch_annex_b(&nal)?;
            let raw = AnnexBSplitter::new(&patched)
                .next()
                .ok_or_else(|| error("no High SPS"))?;
            let sps = Sps::parse(&parse_nal_unit(raw).map_err(error)?.rbsp).map_err(error)?;
            assert_eq!(sps.profile_idc, 100);
            assert_eq!(sps.cropped_dimensions(), (34, 18));
            assert_eq!(sps.vui, Sps::parse(&rbsp).map_err(error)?.vui);
            assert_eq!(layout.patch_annex_b(&patched)?, patched);
            let original = AnnexBSplitter::new(&nal)
                .next()
                .ok_or_else(|| error("no original High SPS"))?;
            let record = AvcDecoderConfigurationRecord::from_parameter_sets(&[original], &[], &[])
                .map_err(error)?
                .to_bytes()
                .map_err(error)?;
            let patched = layout.patch_avcc(&record)?;
            let sps = AvcDecoderConfigurationRecord::parse(&patched)
                .map_err(error)?
                .parsed_sps()
                .map_err(error)?;
            assert_eq!(sps.profile_idc, 100);
            assert_eq!(sps.cropped_dimensions(), (34, 18));
            assert_eq!(sps.vui, Sps::parse(&rbsp).map_err(error)?.vui);
            assert_eq!(layout.patch_avcc(&patched)?, patched);
            assert_eq!(H264Layout::new(48, 32)?.patch_annex_b(&nal)?, nal);
        }
        let unsupported = BaselineSpsConfig {
            width_in_mbs: 3,
            height_in_mbs: 2,
            profile_idc: 100,
            chroma_format_idc: 0,
            ..BaselineSpsConfig::default()
        };
        assert!(
            layout
                .patch_annex_b(&build_nal_unit(
                    3,
                    NalUnitType::Sps,
                    &build_baseline_sps_rbsp(&unsupported)
                ))
                .is_err()
        );
        let mut malformed = Bits::new(&[0, 0, 0, 0, 0]);
        assert!(malformed.ue().is_err());
        Ok(())
    }
    #[test]
    fn production_high_recording_crops_640x360_and_decodes_rgb_pts() -> Result<()> {
        use crate::{RecordingCodec, VideoRecorder, VideoSource};
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "fictrac-high-crop-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory)?;
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        for codec in [RecordingCodec::H264, RecordingCodec::H264Mp4] {
            let filename = directory.join(format!("cropped-high.{}", codec.extension()));
            let mut recorder = VideoRecorder::create(&filename, 640, 360, 30., codec)?;
            for index in 0..2 {
                let pixel = [70 + index * 10, 120, 180];
                recorder.write_rgb(&(0..640 * 360).flat_map(|_| pixel).collect::<Vec<_>>())?;
            }
            recorder.finish()?;
            let bytes = std::fs::read(&filename)?;
            let profile = if codec == RecordingCodec::H264 {
                bytes
                    .windows(5)
                    .find(|w| w[..3] == [0, 0, 1] && w[3] & 31 == 7)
                    .map(|w| w[4])
            } else {
                bytes
                    .windows(4)
                    .position(|w| w == b"avcC")
                    .and_then(|i| bytes.get(i + 5))
                    .copied()
            };
            assert_eq!(
                profile,
                Some(100),
                "production encoder must really emit High profile"
            );
            let mut source = VideoSource::open(&filename)?;
            source.enable_color();
            assert_eq!((source.info.width, source.info.height), (640, 360));
            let mut count = 0;
            while let Some(frame) = source.next_frame()? {
                assert_eq!((frame.width(), frame.height()), (640, 360));
                assert!((frame.timestamp_ms() - count as f64 * 1000. / 30.).abs() < 1e-6);
                let rgb = source
                    .take_rgb()
                    .ok_or_else(|| error("High RGB unavailable"))?;
                assert_eq!(rgb.len(), 640 * 360 * 3);
                let expected = [70 + count * 10, 120, 180];
                for pixel in rgb.as_chunks::<3>().0 {
                    for (actual, wanted) in pixel.iter().zip(expected) {
                        assert!(
                            (*actual as i32 - wanted).abs() <= 12,
                            "High decoded channel {actual} expected {wanted}"
                        );
                    }
                }
                count += 1;
            }
            assert_eq!(count, 2);
        }
        Ok(())
    }
}
