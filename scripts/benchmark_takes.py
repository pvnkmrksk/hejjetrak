#!/usr/bin/env python3
"""Replay native takes through Rust and same-config original FicTrac.

Development benchmark only. Original files are read-only; every generated file
and upstream's rewritten config stay in a new output directory. No fitted lag,
axis transform or scale. Original FicTrac is a baseline, not physical truth.
Partial media is explicitly a prefix, never evidence for the missing interval.
"""
import argparse
from collections import Counter
import csv
import json
import math
from pathlib import Path
import re
import subprocess
import time

from analyze_raw_tracking import dat_rows, quality_rows, sha256, trajectory


def set_field(text, key, value):
    pattern = rf"(?m)^\s*{re.escape(key)}\s*:[^\r\n]*"
    line = f"{key} : {value}"
    return (re.sub(pattern, lambda _: line, text) if re.search(pattern, text)
            else text.rstrip() + "\n" + line + "\n")


def execute(command, directory, log):
    start = time.perf_counter()
    with log.open("w") as stream:
        result = subprocess.run(command, cwd=directory, stdout=stream,
                                stderr=subprocess.STDOUT)
    record = {"command": command, "cwd": str(directory),
              "wall_seconds": time.perf_counter() - start,
              "exit_code": result.returncode}
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}); inspect {log}")
    return record


def effective_config(binary, config):
    requests = [{"id": 1, "command": "open", "args": {"config": str(config)}},
                {"id": 2, "command": "config", "session": 1}]
    result = subprocess.run([str(binary), "agent"], text=True, capture_output=True,
                            input="".join(json.dumps(r) + "\n" for r in requests),
                            check=True)
    responses = [json.loads(line) for line in result.stdout.splitlines()]
    if len(responses) != 2 or not all(r.get("ok") for r in responses):
        raise ValueError(f"cannot inspect effective config: {responses}")
    return responses[1]["result"]["effective"]


def band(reference, candidate, quality, frames):
    measured = [i for i in frames if quality[i]["status"] == "tracked"]
    rms = lambda selected: ([math.sqrt(sum((candidate[i][a] - reference[i][a]) ** 2
                                         for i in selected) / len(selected))
                             for a in (1, 2, 3)] if selected else None)
    return {"frames": len(frames), "tracked": len(measured),
            "tracked_fraction": len(measured) / len(frames) if frames else None,
            "statuses": dict(Counter(quality[i]["status"] for i in frames)),
            "rotation_component_rms_all_decisions": rms(frames),
            "rotation_component_rms_tracked_only_diagnostic": rms(measured)}


def rolling_worst(reference, candidate, quality, frames):
    windows = [band(reference, candidate, quality, frames[j:j + 100])
               for j in range(len(frames) - 99)
               if frames[j + 99] - frames[j] == 99]
    return ([max(w["rotation_component_rms_all_decisions"][axis] for w in windows)
             for axis in range(3)] if windows else None)


def one_take(take, output, binary, original, annotations):
    config_path = take / "epoch-0000-config.txt"
    completion = json.loads((take / "completion.json").read_text())
    media = completion["media_recording"]["raw"]
    video = Path(media["output_path"] if media["state"] == "complete"
                 else media["partial_path"])
    if not video.is_file() or not media["container_finalized"]:
        raise ValueError(f"no finalized source video in {take}")
    input_hash = sha256(video)
    annotation = annotations.get(take.name, {})
    if annotation.get("input_sha256", input_hash) != input_hash:
        raise ValueError(f"occlusion annotations do not match source-video SHA256: {take}")
    text = config_path.read_text()
    source_config_hash = sha256(config_path)
    output.mkdir()
    for key, value in [("src_fn", str(video)), ("do_display", "n"),
                       ("save_raw", "n"), ("save_debug", "n"),
                       ("sock_port", "0"), ("zmq_port", "0"), ("com_port", "")]:
        text = set_field(text, key, value)
    replay_config = output / "config.txt"
    replay_config.write_text(text)
    numerical = effective_config(binary, replay_config)
    settings = numerical["spherical_map"]
    original_dir = output / "fictrac"
    original_dir.mkdir()
    (original_dir / "input.avi").symlink_to(video)
    original_text = text
    # Upstream has different omission defaults. Expand exactly the shared-core
    # values actually loaded above; do not normalize original calibration text.
    fields = {"src_fn": "input.avi", "output_fn": "baseline", "sock_port": -1,
              "q_factor": settings["quality"],
              "opt_max_evals": settings["max_evaluations"],
              "opt_bound": numerical["max_rotation"],
              "opt_tol": settings["tolerance"], "opt_max_err": settings["max_error"],
              "thr_ratio": settings["threshold_ratio"],
              "thr_win_pc": settings["threshold_window"],
              "opt_do_global": "y" if settings["global_search"] else "n",
              "opt_global_evals": settings["global_evaluations"],
              "max_bad_frames": settings["max_bad_frames"]}
    for key, value in fields.items():
        original_text = set_field(original_text, key, str(value))
    copied_config = original_dir / "config.txt"
    copied_config.write_text(original_text)
    original_config_before = sha256(copied_config)
    commands = [execute([str(original), "config.txt"], original_dir,
                        output / "fictrac.log"),
                execute([str(binary), "track", "--config", str(replay_config),
                         "--output", str(output / "rust")], Path.cwd(),
                        output / "rust.log")]
    references = list(original_dir.glob("*.dat"))
    if len(references) != 1:
        raise ValueError(f"expected one original FicTrac data file in {original_dir}")
    reference = dat_rows(references[0])
    candidate = dat_rows(output / "rust/all-frames.dat")
    quality = quality_rows(output / "rust/quality.csv", candidate)
    expected = list(range(media["encoded_frames"]))
    if list(reference) != expected or list(candidate) != expected:
        raise ValueError("reference/candidate/source frame counts or IDs differ")
    numeric_count = completion["media_recording"]["numeric_frames"]
    full_video = media["state"] == "complete" and len(expected) == numeric_count
    excluded = annotation.get("occlusion_intervals", [])
    if any(not (len(x) == 2 and 0 <= x[0] < x[1] <= len(expected)) for x in excluded):
        raise ValueError("occlusion annotations must be valid half-open video-frame intervals")
    clear = [i for i in expected[1:] if not any(a <= i < b for a, b in excluded)]
    norms = {i: math.sqrt(sum(reference[i][axis] ** 2 for axis in (1, 2, 3)))
             for i in clear}
    speed_bands = {name: band(reference, candidate, quality,
                             [i for i in clear if low <= norms[i] < high])
                   for name, low, high in [("quiet", 0., .002), ("ordinary", .002, .04),
                                           ("fast", .04, .08), ("very_fast", .08, math.inf)]}
    fast = band(reference, candidate, quality, [i for i in clear if norms[i] >= .04])
    worst = rolling_worst(reference, candidate, quality, clear)
    unexcluded_worst = rolling_worst(reference, candidate, quality, expected[1:])
    gates = {"complete_video": full_video,
             "fast_tracked_fraction_at_least_99_percent":
                 fast["tracked_fraction"] >= .99 if fast["frames"] >= 100 else None,
             "fast_component_rms_below_020":
                 max(fast["rotation_component_rms_all_decisions"]) < .020
                 if fast["frames"] >= 100 else None,
             "all_clear_100_image_component_rms_below_020":
                 max(worst) < .020 if worst else None}
    live_quality = list(csv.DictReader((take / "epoch-0000-quality.csv").open()))
    return {"take": str(take), "video": str(video), "video_sha256": input_hash,
            "source_config_sha256": source_config_hash,
            "source_config_unchanged": sha256(config_path) == source_config_hash,
            "completion": completion, "video_frames": len(expected),
            "numeric_frames": numeric_count, "full_video_available": full_video,
            "live_statuses": dict(Counter(r["status"] for r in live_quality)),
            "live_first_frame": int(live_quality[0]["frame"]),
            "effective_config": numerical, "occlusion_intervals": excluded,
            "annotation_provenance": annotation.get("provenance"),
            "all_frames_without_exclusions": band(reference, candidate, quality, expected[1:]),
            "worst_unexcluded_100_image_component_rms": unexcluded_worst,
            "clear": band(reference, candidate, quality, clear),
            "fast": fast, "speed_bands": speed_bands,
            "worst_clear_100_image_component_rms": worst,
            "trajectory_all_frames": trajectory(reference, candidate, expected, 0),
            "gates": gates,
            "passes": all(v is True for v in gates.values()),
            "commands": commands, "original_config_before_sha256": original_config_before,
            "original_config_after_sha256": sha256(copied_config),
            "all_frames_sha256": sha256(output / "rust/all-frames.dat"),
            "quality_sha256": sha256(output / "rust/quality.csv"),
            "reference_sha256": sha256(references[0]),
            "scope": "Reference-defined speed bands (rad/image), not physical ground truth. "
                     "No fitted alignment or missing-motion reconstruction. Prefix-only media "
                     "cannot establish performance on the missing numeric interval. Unannotated "
                     "takes include all frames; annotations must be visually selected independently."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--take", type=Path, action="append", required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac"))
    parser.add_argument("--original-fictrac", type=Path, required=True)
    parser.add_argument("--annotations", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--require-pass", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary, original = args.binary.resolve(strict=True), args.original_fictrac.resolve(strict=True)
    annotations = json.loads(args.annotations.read_text()) if args.annotations else {}
    report = {"binary": str(binary), "binary_sha256": sha256(binary),
              "original_fictrac": str(original), "original_sha256": sha256(original),
              "thresholds": {"fast_rad_per_image": .04, "very_fast_rad_per_image": .08,
                             "minimum_fast_tracked_fraction": .99, "component_rms_rad": .020},
              "takes": []}
    for index, source in enumerate(args.take):
        result = one_take(source.resolve(strict=True), output / f"take-{index}",
                          binary, original, annotations)
        report["takes"].append(result)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps({"take": result["take"], "gates": result["gates"],
                          "fast": result["fast"]}), flush=True)
    if args.require_pass and not all(r["passes"] for r in report["takes"]):
        raise SystemExit("benchmark gate failed or unavailable; inspect report.json")


if __name__ == "__main__":
    main()
