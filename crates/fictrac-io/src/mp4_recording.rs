//! Minimal bounded ISO BMFF writer for AVC/MPEG4 Visual with exact rational timing.
use crate::{Error, Result};
use oxideav_core::Packet;
use std::{
    fs::{File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
};
pub(crate) struct Mp4Recording {
    file: File,
    width: u16,
    height: u16,
    rate: u32,
    avcc: Vec<u8>,
    mpeg4: bool,
    samples: Vec<(u32, u32, bool)>,
    mdat: u64,
}
impl Mp4Recording {
    pub fn create_with_codec(
        path: &Path,
        width: usize,
        height: usize,
        rate: u32,
        avcc: Vec<u8>,
        mpeg4: bool,
    ) -> Result<Self> {
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&atom(
            b"ftyp",
            if mpeg4 {
                b"isom\0\0\x02\0isomiso2mp41".to_vec()
            } else {
                b"isom\0\0\x02\0isomiso2avc1mp41".to_vec()
            },
        ))?;
        let mdat = file.stream_position()?;
        file.write_all(b"\0\0\0\0mdat")?;
        Ok(Self {
            file,
            width: width as u16,
            height: height as u16,
            rate,
            avcc,
            mpeg4,
            samples: Vec::new(),
            mdat,
        })
    }
    pub fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        let pos = self.file.stream_position()?;
        if self.samples.len() >= 1_000_000
            || pos + packet.data.len() as u64 + 32_000_000 > u32::MAX as u64
        {
            return Err(Error::Video(
                "MP4 segment reached size/frame limit; start a new recording".into(),
            ));
        }
        self.file.write_all(&packet.data)?;
        self.samples
            .push((pos as u32, packet.data.len() as u32, packet.flags.keyframe));
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        let end = self.file.stream_position()?;
        self.file.seek(SeekFrom::Start(self.mdat))?;
        self.file
            .write_all(&((end - self.mdat) as u32).to_be_bytes())?;
        self.file.seek(SeekFrom::Start(end))?;
        let count = self.samples.len() as u32;
        let duration = count * 1000;
        let movie_duration =
            ((count as u64 * 1_000_000 + self.rate as u64 / 2) / self.rate as u64) as u32;
        let mut mvhd = vec![0; 12];
        be(&mut mvhd, 1000);
        be(&mut mvhd, movie_duration);
        be(&mut mvhd, 0x10000);
        mvhd.extend_from_slice(&[1, 0, 0, 0]);
        mvhd.extend_from_slice(&[0; 8]);
        mvhd.extend_from_slice(&matrix());
        mvhd.extend_from_slice(&[0; 24]);
        be(&mut mvhd, 2);
        let mut tkhd = vec![0, 0, 0, 7];
        tkhd.extend_from_slice(&[0; 8]);
        be(&mut tkhd, 1);
        be(&mut tkhd, 0);
        be(&mut tkhd, movie_duration);
        tkhd.extend_from_slice(&[0; 16]);
        tkhd.extend_from_slice(&matrix());
        be(&mut tkhd, (self.width as u32) << 16);
        be(&mut tkhd, (self.height as u32) << 16);
        let mut mdhd = vec![0; 12];
        be(&mut mdhd, self.rate);
        be(&mut mdhd, duration);
        mdhd.extend_from_slice(&[0x55, 0xc4, 0, 0]);
        let mut hdlr = vec![0; 8];
        hdlr.extend_from_slice(b"vide");
        hdlr.extend_from_slice(&[0; 12]);
        hdlr.extend_from_slice(b"FicTrac Rust\0");
        let mut avc1 = vec![0; 6];
        avc1.extend_from_slice(&1u16.to_be_bytes());
        avc1.extend_from_slice(&[0; 16]);
        avc1.extend_from_slice(&self.width.to_be_bytes());
        avc1.extend_from_slice(&self.height.to_be_bytes());
        be(&mut avc1, 0x480000);
        be(&mut avc1, 0x480000);
        be(&mut avc1, 0);
        avc1.extend_from_slice(&1u16.to_be_bytes());
        avc1.extend_from_slice(&[0; 32]);
        avc1.extend_from_slice(&24u16.to_be_bytes());
        avc1.extend_from_slice(&0xffffu16.to_be_bytes());
        if self.mpeg4 {
            avc1.extend(atom(b"esds", mpeg4_esds(&self.avcc)));
        } else {
            avc1.extend(atom(b"avcC", self.avcc.clone()));
        }
        let mut stsd = vec![0; 4];
        be(&mut stsd, 1);
        stsd.extend(atom(if self.mpeg4 { b"mp4v" } else { b"avc1" }, avc1));
        let mut stts = vec![0; 4];
        be(&mut stts, 1);
        be(&mut stts, count);
        be(&mut stts, 1000);
        let mut stsc = vec![0; 4];
        for value in [1, 1, 1, 1] {
            be(&mut stsc, value);
        }
        let mut stsz = vec![0; 4];
        be(&mut stsz, 0);
        be(&mut stsz, count);
        for (_, size, _) in &self.samples {
            be(&mut stsz, *size);
        }
        let mut stco = vec![0; 4];
        be(&mut stco, count);
        for (offset, _, _) in &self.samples {
            be(&mut stco, *offset);
        }
        let mut stss = vec![0; 4];
        be(
            &mut stss,
            self.samples.iter().filter(|s| s.2).count() as u32,
        );
        for (i, (_, _, key)) in self.samples.iter().enumerate() {
            if *key {
                be(&mut stss, i as u32 + 1);
            }
        }
        let mut stbl = Vec::new();
        for (kind, bytes) in [
            (b"stsd", stsd),
            (b"stts", stts),
            (b"stsc", stsc),
            (b"stsz", stsz),
            (b"stco", stco),
            (b"stss", stss),
        ] {
            stbl.extend(atom(kind, bytes));
        }
        let mut dref = vec![0; 4];
        be(&mut dref, 1);
        dref.extend(atom(b"url ", vec![0, 0, 0, 1]));
        let mut minf = atom(b"vmhd", vec![0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        minf.extend(atom(b"dinf", atom(b"dref", dref)));
        minf.extend(atom(b"stbl", stbl));
        let mut mdia = atom(b"mdhd", mdhd);
        mdia.extend(atom(b"hdlr", hdlr));
        mdia.extend(atom(b"minf", minf));
        let mut trak = atom(b"tkhd", tkhd);
        trak.extend(atom(b"mdia", mdia));
        let mut moov = atom(b"mvhd", mvhd);
        moov.extend(atom(b"trak", trak));
        self.file.write_all(&atom(b"moov", moov))?;
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(())
    }
}
fn be(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}
fn atom(kind: &[u8; 4], body: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    be(&mut out, (body.len() + 8) as u32);
    out.extend_from_slice(kind);
    out.extend(body);
    out
}
fn matrix() -> Vec<u8> {
    let mut out = Vec::new();
    for value in [0x10000, 0, 0, 0, 0x10000, 0, 0, 0, 0x40000000] {
        be(&mut out, value);
    }
    out
}

fn descriptor(tag: u8, body: Vec<u8>) -> Vec<u8> {
    let mut length = body.len();
    let mut digits = vec![(length & 127) as u8];
    length >>= 7;
    while length > 0 {
        digits.push((length & 127) as u8 | 128);
        length >>= 7;
    }
    let mut out = vec![tag];
    out.extend(digits.into_iter().rev());
    out.extend(body);
    out
}
fn mpeg4_esds(config: &[u8]) -> Vec<u8> {
    let mut decoder = vec![0x20, 0x11, 0, 0, 0];
    decoder.extend_from_slice(&[0; 8]);
    decoder.extend(descriptor(5, config.to_vec()));
    let mut es = vec![0, 1, 0];
    es.extend(descriptor(4, decoder));
    es.extend(descriptor(6, vec![2]));
    let mut full_box = vec![0; 4];
    full_box.extend(descriptor(3, es));
    full_box
}
