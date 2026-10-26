"""Compare selected actual tracking-source RGB images with independent FFmpeg.

Development oracle only. The Rust app still decodes the complete AVI itself;
FFmpeg is invoked outside the app to inspect exactly the same recorded images.
No enhancement, sharpening, resizing, transcoding or frame interpolation occurs.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess

from benchmark_vr3 import STEM, set_field, sha256

FRAMES = (0, 1695, 1700, 1705, 1750, 1800, 2000, 4810)


def ppm(path):
    data = path.read_bytes()
    head, width_height, maximum, pixels = data.split(b"\n", 3)
    if head != b"P6" or width_height != b"320 240" or maximum != b"255" or len(pixels) != 320 * 240 * 3:
        raise ValueError(f"expected an unmodified 320x240 RGB PPM at {path}")
    return pixels


def gradient_energy(pixels):
    # Green equals luminance for these monochrome images; use the same channel
    # in both decoders to avoid introducing a third color conversion.
    values = pixels[1::3]
    horizontal = sum(abs(values[y * 320 + x] - values[y * 320 + x - 1])
                     for y in range(240) for x in range(1, 320))
    vertical = sum(abs(values[y * 320 + x] - values[(y - 1) * 320 + x])
                   for y in range(1, 240) for x in range(320))
    return (horizontal + vertical) / (240 * 319 + 239 * 320)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac"))
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=False)
    raw = args.reference_dir.resolve() / f"fictrac-raw-{STEM}.avi"
    text = (args.reference_dir / "config.txt").read_text()
    for key, value in [("save_raw", "n"), ("save_debug", "n"), ("sock_port", "0"),
                       ("zmq_port", "0"), ("com_port", "")]:
        text = set_field(text, key, value)
    config = out / "config.txt"
    config.write_text(text)
    requests = [{"command": "open", "args": {"config": str(config), "input": str(raw)}}]
    processed = 0
    for frame in FRAMES:
        requests.append({"command": "run", "session": 1, "args": {"frames": frame + 1 - processed}})
        requests.append({"command": "snapshot", "session": 1, "args": {"directory": str(out / f"rust-{frame:04d}")}})
        processed = frame + 1
    lines = "".join(json.dumps({"id": i, **request}) + "\n" for i, request in enumerate(requests, 1))
    (out / "requests.jsonl").write_text(lines)
    result = subprocess.run([str(args.binary.resolve()), "agent"], input=lines,
                            capture_output=True, text=True, check=True)
    (out / "responses.jsonl").write_text(result.stdout)
    responses = [json.loads(line) for line in result.stdout.splitlines()]
    if len(responses) != len(requests) or any(not response["ok"] for response in responses):
        raise RuntimeError("Rust snapshot request failed; inspect responses.jsonl")
    for index, frame in enumerate(FRAMES):
        if responses[2 + 2 * index]["result"]["frame"]["frame"] != frame:
            raise RuntimeError("snapshot frame identity differs from requested recorded-video image")
    selected = "+".join(f"eq(n\\,{frame})" for frame in FRAMES)
    command = ["ffmpeg", "-v", "error", "-i", str(raw), "-vf", "select=" + selected,
               "-vsync", "0", "-pix_fmt", "rgb24", str(out / "ffmpeg-%02d.ppm")]
    reference = subprocess.run(command, capture_output=True, text=True, check=True)
    (out / "ffmpeg-stderr.log").write_text(reference.stderr)
    if reference.stderr.strip():
        raise RuntimeError("FFmpeg reported a decode error; inspect ffmpeg-stderr.log")
    report = {"input_sha256": sha256(raw), "binary_sha256": sha256(args.binary),
              "scope": __doc__, "ffmpeg_command": command,
              "ffmpeg_version": subprocess.check_output(["ffmpeg", "-version"], text=True).splitlines()[0],
              "frames": []}
    for number, frame in enumerate(FRAMES, 1):
        rust = ppm(out / f"rust-{frame:04d}" / "color-input.ppm")
        ffmpeg = ppm(out / f"ffmpeg-{number:02d}.ppm")
        differences = [int(a) - int(b) for a, b in zip(rust, ffmpeg)]
        rmse = math.sqrt(sum(value * value for value in differences) / len(differences))
        rust_gradient, ffmpeg_gradient = gradient_energy(rust), gradient_energy(ffmpeg)
        report["frames"].append({"video_frame": frame,
                                 "rust_rgb_sha256": hashlib.sha256(rust).hexdigest(),
                                 "ffmpeg_rgb_sha256": hashlib.sha256(ffmpeg).hexdigest(),
                                 "rgb_rmse_levels_0_to_255": rmse,
                                 "max_absolute_rgb_error": max(abs(value) for value in differences),
                                 "rust_mean_neighbor_gradient": rust_gradient,
                                 "ffmpeg_mean_neighbor_gradient": ffmpeg_gradient,
                                 "gradient_ratio_rust_over_ffmpeg": rust_gradient / ffmpeg_gradient if ffmpeg_gradient else None,
                                 "matches_within_rounding_gate": rmse <= 1.5 and max(abs(value) for value in differences) <= 3})
    report["passes_selected_image_rounding_gate"] = all(frame["matches_within_rounding_gate"] for frame in report["frames"])
    (out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    if not report["passes_selected_image_rounding_gate"]:
        raise RuntimeError("selected decoded images differ beyond rounding; inspect the retained pixels before claiming decoder parity")


if __name__ == "__main__":
    main()
