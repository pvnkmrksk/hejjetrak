//! Build a bounded grayscale, uncompressed resource fixture without subprocesses.
//!
//! Decode each source frame once through the same RGB-to-gray adapter used by
//! legacy tracking. Cache its exact Y4M packet, then emit a continuous ping-pong
//! sequence at120FPS:0,1,...,N-1,N-2,...,1, repeated CYCLES times. Reversing at
//! adjacent source frames avoids the artificial last-to-first pose jump of a
//! looped clip. This is a resource workload, not physical-camera or RGB evidence.
//! Packet cache (including Vec metadata) is capped at512MiB; output at2GiB.
use fictrac_io::{VideoSource, write_y4m_frame, write_y4m_header};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::PathBuf,
};

const CACHE_LIMIT: usize = 512 * 1024 * 1024;
const OUTPUT_LIMIT: u64 = 2 * 1024 * 1024 * 1024;
const FPS: u32 = 120;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 3 {
        return Err("usage: resource_fixture VIDEO CYCLES NEW_OUTPUT.y4m (cycles1..30)".into());
    }
    let input = PathBuf::from(&arguments[0]);
    let cycles: usize = arguments[1]
        .to_str()
        .ok_or("CYCLES must be integer text in1..30")?
        .parse()
        .map_err(|_| "CYCLES must be an integer in1..30")?;
    if !(1..=30).contains(&cycles) {
        return Err("CYCLES must be in1..30".into());
    }
    let output = PathBuf::from(&arguments[2]);
    if output.exists() {
        return Err(format!("output must be new: {}", output.display()).into());
    }
    let mut source = VideoSource::open(&input)?;
    let info = source.info.clone();
    let packet_capacity = info
        .width
        .checked_mul(info.height)
        .and_then(|pixels| pixels.checked_add(b"FRAME\n".len()))
        .ok_or("source packet dimensions overflow")?;
    let per_frame = packet_capacity
        .checked_add(std::mem::size_of::<Vec<u8>>())
        .ok_or("source packet metadata dimensions overflow")?;
    let maximum_frames = CACHE_LIMIT / per_frame;
    if maximum_frames < 2 {
        return Err("source dimensions cannot fit two packets within512MiB cache".into());
    }
    let mut packets: Vec<Vec<u8>> = Vec::new();
    let mut packet_bytes = 0usize;
    while let Some(frame) = source.next_tracking_frame(None)? {
        if frame.width() != info.width || frame.height() != info.height {
            return Err("source frame dimensions changed during decode".into());
        }
        if packets.len() >= maximum_frames {
            return Err("input exceeds512MiB packet cache; use a shorter source clip".into());
        }
        if packets.len() == packets.capacity() {
            let capacity = packets
                .capacity()
                .saturating_mul(2)
                .max(8)
                .min(maximum_frames);
            packets
                .try_reserve_exact(capacity - packets.len())
                .map_err(|error| format!("cannot allocate packet metadata: {error}"))?;
        }
        let mut packet = Vec::new();
        packet
            .try_reserve_exact(packet_capacity)
            .map_err(|error| format!("cannot allocate frame packet: {error}"))?;
        write_y4m_frame(&mut packet, &frame)?;
        packet_bytes = packet_bytes
            .checked_add(packet.capacity())
            .ok_or("cache size overflow")?;
        let cache_bytes = packets
            .capacity()
            .checked_mul(std::mem::size_of::<Vec<u8>>())
            .and_then(|metadata| metadata.checked_add(packet_bytes))
            .ok_or("cache metadata size overflow")?;
        if cache_bytes > CACHE_LIMIT {
            return Err("input exceeds512MiB packet cache; use a shorter source clip".into());
        }
        packets.push(packet);
        // Color was required for conversion, but is not retained in the cache.
        drop(source.take_rgb());
    }
    drop(source);
    if packets.len() < 2 {
        return Err("input must contain at least two decoded frames".into());
    }
    let mut header = Vec::new();
    write_y4m_header(&mut header, info.width, info.height, FPS)?;
    let mut cycle_bytes = 0u64;
    for index in (0..packets.len()).chain((1..packets.len() - 1).rev()) {
        cycle_bytes = cycle_bytes
            .checked_add(packets[index].len() as u64)
            .ok_or("output cycle size overflow")?;
    }
    let output_bytes = cycle_bytes
        .checked_mul(cycles as u64)
        .and_then(|bytes| bytes.checked_add(header.len() as u64))
        .ok_or("output size overflow")?;
    if output_bytes > OUTPUT_LIMIT {
        return Err(format!(
            "output would be{output_bytes}bytes, exceeding2GiB; reduce CYCLES or source size"
        )
        .into());
    }
    let cycle_frames = packets
        .len()
        .checked_mul(2)
        .and_then(|count| count.checked_sub(2))
        .ok_or("output frame count overflow")?;
    let output_frames = cycle_frames
        .checked_mul(cycles)
        .ok_or("output frame count overflow")?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .map_err(|error| format!("cannot create new fixture {}: {error}", output.display()))?;
    let mut writer = BufWriter::new(file);
    writer.write_all(&header)?;
    for _ in 0..cycles {
        for index in (0..packets.len()).chain((1..packets.len() - 1).rev()) {
            writer.write_all(&packets[index])?;
        }
    }
    writer.flush()?;
    writer.get_ref().sync_all()?;
    let actual_bytes = writer.get_ref().metadata()?.len();
    if actual_bytes != output_bytes {
        return Err(format!(
            "fixture length mismatch: expected{output_bytes}, wrote{actual_bytes}"
        )
        .into());
    }
    let cache_bytes = packet_bytes + packets.capacity() * std::mem::size_of::<Vec<u8>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "input":input.to_string_lossy(), "output":output.to_string_lossy(), "source_info":info,
            "unique_frames":packets.len(), "cycles":cycles, "frames_per_cycle":cycle_frames,
            "output_frames":output_frames, "fps":FPS, "output_bytes":output_bytes,
            "cached_packet_bytes":packet_bytes, "cache_with_metadata_bytes":cache_bytes,
            "cache_limit_bytes":CACHE_LIMIT, "output_limit_bytes":OUTPUT_LIMIT,
            "sequence":"0,1,...,N-1,N-2,...,1; repeated without a last-to-first jump",
            "pixels":"exact legacy RGB-to-gray8bit bytes through next_tracking_frame(None) and write_y4m_frame",
            "scope":"development-only grayscale resource fixture; source timing replaced by120FPS; not RGB capture or physical accuracy evidence"
        }))?
    );
    Ok(())
}
