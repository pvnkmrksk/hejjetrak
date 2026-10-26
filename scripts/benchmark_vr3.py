"""Align the private VR3 recording to FicTrac's source-frame IDs and audit recovery.

Development analysis only: Python/ffprobe are never application dependencies.
The private recording stays outside the repository. Example:

  python3 scripts/benchmark_vr3.py --reference-dir /path/to/VR3 \
      --candidate-dir runs/vr3 --output runs/vr3-comparison.json

Use --binary target/release/fictrac to produce candidate-dir first. This
copies the original config and changes only recording/output switches; numerical
settings and all four physical calibration files are left untouched.
"""

import argparse
from collections import Counter
import csv
import hashlib
import json
import math
from pathlib import Path
import platform
import re
import statistics
import subprocess
import time


STEM = "20261026_212053"
# Video-frame intervals, selected by visual inspection of the source video.
# They are intentionally independent of candidate quality decisions.
HEALTHY = (2, 4680)
# The visible acceleration and subsequent fast rotation, annotated from the raw
# recording before measuring the final candidate. Rolling checks below also
# cover every other clear pre-hand interval.
FAST = (1690, 2100)
HAND = (4690, 4820)
POST = (4820, 5157)


def dat_rows(path):
    rows = []
    with path.open(newline="") as stream:
        for number, fields in enumerate(csv.reader(stream), 1):
            values = [float(value) for value in fields]
            if len(values) != 25 or not all(math.isfinite(v) for v in values):
                raise ValueError(f"{path}:{number}: expected 25 finite columns")
            if values[0] != int(values[0]):
                raise ValueError(f"{path}:{number}: non-integer frame ID")
            rows.append(values)
    return rows


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def set_field(text, key, value):
    pattern = rf"(?m)^(\s*{re.escape(key)}\s*:)\s*[^\r\n]*"
    if re.search(pattern, text):
        return re.sub(pattern, lambda m: m[1] + " " + value, text)
    return text.rstrip() + f"\n{key} : {value}\n"


def interval_quality(quality, bounds):
    selected = quality[bounds[0]:bounds[1]]
    accepted = sum(row["accepted"] == "true" for row in selected)
    errors = sorted(float(row["fit_error"]) for row in selected)
    accepted_errors = sorted(float(row["fit_error"]) for row in selected if row["accepted"] == "true")
    return {
        "video_frame_interval": list(bounds),
        "frames": len(selected),
        "accepted": accepted,
        "accepted_fraction": accepted / len(selected),
        "statuses": dict(Counter(row["status"] for row in selected)),
        "fit_error_median": statistics.median(errors),
        "fit_error_p95": errors[math.ceil(len(errors) * 0.95) - 1],
        "accepted_fit_error_p95": accepted_errors[math.ceil(len(accepted_errors) * 0.95) - 1] if accepted_errors else None,
    }


def compare(reference, candidate, mapping, quality, bounds):
    # A candidate delta spans two recorded images. A source delta spans two
    # capture images. They are comparable only when their source IDs differ by 1.
    indices = [i for i in range(*bounds) if mapping[i] - mapping[i - 1] == 1]
    differences = [[candidate[i][j] - reference[mapping[i]][j] for j in (1, 2, 3)]
                   for i in indices]
    rmse = [math.sqrt(sum(d[j] ** 2 for d in differences) / len(indices))
            for j in range(3)]
    accepted_indices = [i for i in indices if quality[i]["status"] == "tracked"]
    accepted_rmse = [math.sqrt(sum((candidate[i][j] - reference[mapping[i]][j]) ** 2
                                 for i in accepted_indices) / len(accepted_indices))
                     for j in (1, 2, 3)] if accepted_indices else None
    path_squared = [(candidate[i][14] - reference[mapping[i]][14]) ** 2
                    + (candidate[i][15] - reference[mapping[i]][15]) ** 2 for i in indices]
    heading_squared = []
    for i in indices:
        difference = candidate[i][16] - reference[mapping[i]][16]
        heading_squared.append(math.atan2(math.sin(difference), math.cos(difference)) ** 2)
    result = interval_quality(quality, bounds)
    reference_start = min(reference)
    reference_end = mapping[bounds[1] - 1]
    reference_length = sum(math.hypot(reference[i][14] - reference[i - 1][14],
                                     reference[i][15] - reference[i - 1][15])
                           for i in range(reference_start + 1, reference_end + 1))
    reference_displacement = math.hypot(reference[reference_end][14] - reference[reference_start][14],
                                        reference[reference_end][15] - reference[reference_start][15])
    path_rmse = math.sqrt(sum(path_squared) / len(indices))
    result.update({
        "aligned_delta_frames": len(indices),
        "excluded_source_gap_video_frames": [i for i in range(*bounds) if mapping[i] - mapping[i - 1] != 1],
        "rotation_component_rmse_rad_all_decisions": rmse,
        "rotation_component_mean_error_rad_all_decisions":
            [statistics.mean(d[j] for d in differences) for j in range(3)],
        "lab_rotation_component_mean_error_rad_all_decisions":
            [statistics.mean(candidate[i][j] - reference[mapping[i]][j] for i in indices)
             for j in (5, 6, 7)],
        "tracked_only_delta_frames": len(accepted_indices),
        "rotation_component_rmse_rad_tracked_only_diagnostic": accepted_rmse,
        "path_point_rmse_sphere_rad": path_rmse,
        "heading_rmse_rad": math.sqrt(sum(heading_squared) / len(indices)),
        "endpoint_error_sphere_rad": math.sqrt(path_squared[-1]),
        "reference_path_context_from_source_initialization": {
            "source_frame_interval_inclusive": [reference_start, reference_end],
            "reference_path_length_sphere_rad": reference_length,
            "reference_endpoint_displacement_sphere_rad": reference_displacement,
            "path_rmse_divided_by_reference_path_length": path_rmse / reference_length if reference_length else None,
            "path_rmse_divided_by_reference_endpoint_displacement": path_rmse / reference_displacement if reference_displacement else None,
            "note": "Descriptive scale context only. These ratios do not replace raw errors or establish long-sequence path parity.",
        },
    })
    return result


def common_windows(reference, candidate, mapping, quality, bounds, length=100):
    windows = []
    excluded = []
    for begin in range(bounds[0], bounds[1] - length + 1, length):
        end = begin + length
        if any(quality[i]["status"] != "tracked" for i in range(begin, end)):
            excluded.append({"video_frame_interval": [begin, end], "reason": "untracked image"})
            continue
        if any(mapping[i] != mapping[i - 1] + 1 for i in range(begin + 1, end)):
            excluded.append({"video_frame_interval": [begin, end], "reason": "missing source image"})
            continue
        ref0 = reference[mapping[begin]]
        cand0 = candidate[begin]
        angle = ref0[16] - cand0[16]
        cosine, sine = math.cos(angle), math.sin(angle)
        paths = []
        translated_paths = []
        headings = []
        for i in range(begin + 1, end):
            ref = reference[mapping[i]]
            cand = candidate[i]
            cx, cy = cand[14] - cand0[14], cand[15] - cand0[15]
            rx, ry = ref[14] - ref0[14], ref[15] - ref0[15]
            paths.append((cosine * cx - sine * cy - rx) ** 2 +
                         (sine * cx + cosine * cy - ry) ** 2)
            translated_paths.append((cx - rx) ** 2 + (cy - ry) ** 2)
            dh = (cand[16] - cand0[16]) - (ref[16] - ref0[16])
            headings.append(math.atan2(math.sin(dh), math.cos(dh)) ** 2)
        windows.append({
            "video_frame_interval": [begin, end],
            "translated_path_rmse_sphere_rad": math.sqrt(sum(translated_paths) / len(translated_paths)),
            "start_heading_aligned_path_rmse_sphere_rad": math.sqrt(sum(paths) / len(paths)),
            "relative_heading_rmse_rad": math.sqrt(sum(headings) / len(headings)),
        })
    summary = {}
    for field in ["translated_path_rmse_sphere_rad", "start_heading_aligned_path_rmse_sphere_rad",
                  "relative_heading_rmse_rad"]:
        values = sorted(window[field] for window in windows)
        summary[field] = {"median": statistics.median(values),
                          "p95": values[math.ceil(len(values) * 0.95) - 1], "max": max(values)} if values else None
    return {
        "images_per_window": length, "windows": windows, "excluded_windows": excluded,
        "summary": summary,
        "note": "Diagnostics on fixed non-overlapping windows with 100 common, consecutively captured, tracked images. Translation removes starting path offset; heading alignment additionally rotates the candidate path by its starting heading discrepancy. These metrics do not replace all-frame gates or global path drift.",
    }


def interval_trajectory(reference, candidate, mapping, bounds):
    """Report continuous interval drift without fitting away its later errors."""
    begin, end = bounds
    ref0, cand0 = reference[mapping[begin]], candidate[begin]
    angle = ref0[16] - cand0[16]
    cosine, sine = math.cos(angle), math.sin(angle)
    paths, headings = [], []
    for i in range(begin, end):
        ref, cand = reference[mapping[i]], candidate[i]
        cx, cy = cand[14] - cand0[14], cand[15] - cand0[15]
        rx, ry = ref[14] - ref0[14], ref[15] - ref0[15]
        paths.append((cosine * cx - sine * cy - rx) ** 2
                     + (sine * cx + cosine * cy - ry) ** 2)
        heading = (cand[16] - cand0[16]) - (ref[16] - ref0[16])
        headings.append(math.atan2(math.sin(heading), math.cos(heading)) ** 2)
    return {
        "video_frame_interval": list(bounds),
        "start_position_and_heading_aligned_path_rmse_sphere_rad": math.sqrt(statistics.mean(paths)),
        "start_position_and_heading_aligned_endpoint_error_sphere_rad": math.sqrt(paths[-1]),
        "relative_heading_rmse_rad": math.sqrt(statistics.mean(headings)),
        "note": "Only the initial position and heading are aligned; no later time-dependent rotation, scaling, trajectory fitting or gap reconstruction is applied. These drift metrics are reported independently of increment gates.",
    }


def rolling_motion(reference, candidate, mapping, quality, bounds, length=100):
    """Keep local failures visible in every overlapping one-second interval."""
    windows = []
    excluded = []
    for begin in range(bounds[0], bounds[1] - length + 1):
        end = begin + length
        if any(mapping[i] != mapping[i - 1] + 1 for i in range(begin, end)):
            excluded.append(begin)
            continue
        rmse = [math.sqrt(sum((candidate[i][j] - reference[mapping[i]][j]) ** 2
                              for i in range(begin, end)) / length) for j in (1, 2, 3)]
        windows.append({
            "video_frame_interval": [begin, end],
            "accepted": sum(quality[i]["accepted"] == "true" for i in range(begin, end)),
            "rotation_component_rmse_rad_all_decisions": rmse,
        })
    worst = [max(windows, key=lambda window: window["rotation_component_rmse_rad_all_decisions"][axis])
             for axis in range(3)]
    return {
        "images_per_window": length,
        "windows": windows,
        "excluded_window_starts_for_missing_source_images": excluded,
        "worst_component_rmse_rad": [window["rotation_component_rmse_rad_all_decisions"][axis]
                                     for axis, window in enumerate(worst)],
        "worst_component_video_frame_intervals": [window["video_frame_interval"] for window in worst],
        "note": "Every overlapping 100-image clear pre-hand window with consecutive source IDs is checked, including unaccepted candidate decisions. Only unavailable source-image deltas are excluded, and their window starts are retained. This supplements the full-interval gate.",
    }


def upstream_interval(reference, mapping, bounds):
    rows = [reference[mapping[i]] for i in range(*bounds)]
    errors = sorted(row[4] for row in rows)
    norms = sorted(math.sqrt(sum(value * value for value in row[1:4])) for row in rows)
    return {
        "video_frame_interval": list(bounds),
        "fit_error_median": statistics.median(errors),
        "fit_error_p95": errors[math.ceil(len(errors) * 0.95) - 1],
        "delta_norm_median_rad": statistics.median(norms),
        "delta_norm_p95_rad": norms[math.ceil(len(norms) * 0.95) - 1],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--suffix-control-dir", type=Path,
                        help="optional independent fresh-atlas data recording starting at video frame 4820")
    parser.add_argument("--upstream-replay-dat", type=Path,
                        help="optional OG .dat from replaying this same AVI, with frame IDs 0..5156")
    parser.add_argument("--make-suffix-control", action="store_true",
                        help="with --binary, generate a NEW --suffix-control-dir using the same decoder")
    parser.add_argument("--require-pass", action="store_true")
    # Existing sample rotation gate, declared before measuring the new fixture.
    parser.add_argument("--max-rotation-rmse", type=float, default=0.020)
    parser.add_argument("--min-healthy-acceptance", type=float, default=0.95)
    parser.add_argument("--min-post-acceptance", type=float, default=0.90)
    args = parser.parse_args()
    if args.make_suffix_control and (not args.binary or not args.suffix_control_dir):
        parser.error("--make-suffix-control requires --binary and --suffix-control-dir")
    if args.make_suffix_control and args.suffix_control_dir.exists():
        parser.error("--make-suffix-control requires a new --suffix-control-dir")
    reference_dir = args.reference_dir.resolve()
    candidate_dir = args.candidate_dir.resolve()
    raw = reference_dir / f"fictrac-raw-{STEM}.avi"
    reference_path = reference_dir / f"fictrac-{STEM}.dat"
    mapping_path = reference_dir / f"fictrac-vidLogFrames-{STEM}.txt"
    run_wall = None
    if args.binary:
        if candidate_dir.exists():
            parser.error("--binary requires a new --candidate-dir")
        candidate_dir.parent.mkdir(parents=True, exist_ok=True)
        config = (reference_dir / "config.txt").read_text()
        for key, value in [("save_raw", "n"), ("save_debug", "n"), ("sock_port", "0"),
                           ("zmq_port", "0"), ("com_port", "")]:
            config = set_field(config, key, value)
        config_path = candidate_dir.parent / (candidate_dir.name + "-config.txt")
        with config_path.open("x") as stream:
            stream.write(config)
        started = time.perf_counter()
        completed = subprocess.run([
            str(args.binary.resolve()), "track", "--config", str(config_path),
            "--input", str(raw), "--output", str(candidate_dir),
        ], capture_output=True, text=True, check=True)
        run_wall = time.perf_counter() - started
        (candidate_dir / "console.log").write_text(completed.stdout + completed.stderr)
        if args.make_suffix_control:
            control_dir = args.suffix_control_dir.resolve()
            requests = [
                {"command": "open", "args": {"config": str(config_path), "input": str(raw)}},
                {"command": "run", "session": 1, "args": {"frames": POST[0]}},
                {"command": "reset", "session": 1},
                {"command": "record_start", "session": 1,
                 "args": {"directory": str(control_dir), "record_options": {"raw": False, "debug": False}}},
                {"command": "run", "session": 1, "args": {"frames": POST[1] - POST[0]}},
                {"command": "record_stop", "session": 1},
            ]
            request_text = "".join(json.dumps({"id": i, **request}) + "\n" for i, request in enumerate(requests, 1))
            (candidate_dir / "suffix-control-requests.jsonl").write_text(request_text)
            completed = subprocess.run([str(args.binary.resolve()), "agent"], input=request_text,
                                       capture_output=True, text=True, check=True)
            (candidate_dir / "suffix-control-responses.jsonl").write_text(completed.stdout)
            responses = [json.loads(line) for line in completed.stdout.splitlines()]
            if len(responses) != len(requests) or any(not response.get("ok") for response in responses):
                raise ValueError("suffix control command failed; inspect suffix-control-responses.jsonl")
    candidate = dat_rows(candidate_dir / "all-frames.dat")
    upstream = dat_rows(reference_path)
    reference = {int(row[0]): row for row in upstream}
    mapping = [int(value) for value in mapping_path.read_text().splitlines()]
    with (candidate_dir / "quality.csv").open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    if len(candidate) != len(mapping) or len(quality) != len(mapping) or len(mapping) != POST[1]:
        raise ValueError("expected the complete 5,157-frame benchmark and matching all-frame quality")
    for i, (row, decision) in enumerate(zip(candidate, quality)):
        if row[0] != i or int(decision["frame"]) != i or mapping[i] not in reference:
            raise ValueError(f"unaligned frame at video index {i}")
        if i and mapping[i] <= mapping[i - 1]:
            raise ValueError("source frame mapping must increase strictly")
    healthy = compare(reference, candidate, mapping, quality, HEALTHY)
    fast = compare(reference, candidate, mapping, quality, FAST)
    fast["interval_trajectory"] = interval_trajectory(reference, candidate, mapping, FAST)
    rolling = rolling_motion(reference, candidate, mapping, quality, HEALTHY)
    post = interval_quality(quality, POST)
    relocalized = [i for i in range(HAND[0], POST[1]) if quality[i]["status"] == "relocalized"]
    zero_motion_violations = [i for i, (row, decision) in enumerate(zip(candidate, quality))
                              if (decision["accepted"] != "true" or decision["status"] == "relocalized")
                              and any(abs(row[j]) > 1e-10 for j in (1, 2, 3))]
    moving = [math.sqrt(sum(candidate[i][j] ** 2 for j in (1, 2, 3)))
              for i in range(*POST) if quality[i]["status"] == "tracked"]
    post["tracked_delta_norm_rms_rad"] = math.sqrt(sum(v * v for v in moving) / max(1, len(moving)))
    gates = {
        "healthy_rotation_component_rmse": all(v <= args.max_rotation_rmse for v in
            healthy["rotation_component_rmse_rad_all_decisions"]),
        "healthy_acceptance": healthy["accepted_fraction"] >= args.min_healthy_acceptance,
        "every_rolling_100_image_rotation_component_rmse":
            all(v <= args.max_rotation_rmse for v in rolling["worst_component_rmse_rad"]),
        "fast_section_rotation_component_rmse":
            all(v <= args.max_rotation_rmse for v in fast["rotation_component_rmse_rad_all_decisions"]),
        "fast_section_every_image_tracked": fast["statuses"] == {"tracked": fast["frames"]},
        "post_hand_acceptance": post["accepted_fraction"] >= args.min_post_acceptance,
        "confirmed_relocalization_after_hand_arrival": bool(relocalized),
        "missing_interval_and_relocalization_have_zero_relative_motion": not zero_motion_violations,
        "post_hand_is_not_a_frozen_zero_estimate": post["tracked_delta_norm_rms_rad"] > 0.001,
        "post_hand_fit_error_p95_within_twice_accepted_healthy":
            healthy["accepted_fit_error_p95"] is not None and
            post["fit_error_p95"] <= 2 * healthy["accepted_fit_error_p95"],
    }
    suffix_comparison = None
    if args.suffix_control_dir:
        control_rows = dat_rows(args.suffix_control_dir / "all-frames.dat")
        control = {int(row[0]): row for row in control_rows}
        with (args.suffix_control_dir / "quality.csv").open(newline="") as stream:
            control_quality = list(csv.DictReader(stream))
        if (sorted(control) != list(range(*POST)) or len(control_rows) != POST[1] - POST[0]
                or len(control_quality) != len(control_rows)
                or any(int(decision["frame"]) != int(row[0])
                       for row, decision in zip(control_rows, control_quality))):
            raise ValueError("suffix control must contain exactly video frames 4820..5156")
        settled = (4920, POST[1])
        suffix_rmse = [math.sqrt(sum((candidate[i][j] - control[i][j]) ** 2
                                   for i in range(*settled)) / (settled[1] - settled[0]))
                       for j in (1, 2, 3)]
        suffix_comparison = {
            "control": str(args.suffix_control_dir.resolve()),
            "control_initialization_video_frame": POST[0],
            "comparison_video_frame_interval": list(settled),
            "control_frames": len(control_rows),
            "control_accepted": sum(row["accepted"] == "true" for row in control_quality),
            "rotation_component_rmse_rad": suffix_rmse,
            "note": "Same decoded images, independently initialized atlas. Agreement tests recovered increments; it is not physical ground truth.",
        }
        gates["recovered_increments_match_fresh_suffix_control"] = all(v <= args.max_rotation_rmse for v in suffix_rmse)
        gates["fresh_suffix_control_accepts_every_image"] = suffix_comparison["control_accepted"] == len(control_rows)
    same_video_comparison = None
    if args.upstream_replay_dat:
        replay_rows = dat_rows(args.upstream_replay_dat)
        if len(replay_rows) != len(candidate) or any(row[0] != i for i, row in enumerate(replay_rows)):
            raise ValueError("upstream replay must contain exactly recorded-video frame IDs 0..5156")
        replay = {int(row[0]): row for row in replay_rows}
        replay_mapping = list(range(len(replay_rows)))
        same_healthy = compare(replay, candidate, replay_mapping, quality, HEALTHY)
        same_fast = compare(replay, candidate, replay_mapping, quality, FAST)
        same_fast["interval_trajectory"] = interval_trajectory(replay, candidate, replay_mapping, FAST)
        same_rolling = rolling_motion(replay, candidate, replay_mapping, quality, HEALTHY)
        same_video_comparison = {
            "reference": str(args.upstream_replay_dat.resolve()),
            "reference_sha256": sha256(args.upstream_replay_dat),
            "healthy_pre_hand": same_healthy,
            "fast_section": same_fast,
            "rolling_healthy_100_image_rotation": same_rolling,
            "note": "OG was replayed on the same compressed AVI. All frames align directly; no missing-source-frame mapping is needed. Increment gates use the same 0.020 rad/component limit. Long-path errors remain reported independently, and failed upstream post-hand output is not used as a ground-truth oracle.",
        }
        for name, values in [
            ("same_recording_healthy_rotation_component_rmse", same_healthy["rotation_component_rmse_rad_all_decisions"]),
            ("same_recording_fast_rotation_component_rmse", same_fast["rotation_component_rmse_rad_all_decisions"]),
            ("same_recording_every_rolling_100_image_rotation_component_rmse", same_rolling["worst_component_rmse_rad"]),
        ]:
            gates[name] = all(value <= args.max_rotation_rmse for value in values)
    report = {
        "fixture": "VR3 20261026_212053", "platform": platform.platform(),
        "input_sha256": sha256(raw), "reference_dat_sha256": sha256(reference_path),
        "source_frame_map_sha256": sha256(mapping_path),
        "video_frames": len(mapping), "reference_rows": len(upstream),
        "source_gap_video_frames": [i for i in range(1, len(mapping)) if mapping[i] != mapping[i - 1] + 1],
        "interval_basis": "Conservative video-frame intervals from visual inspection; independent of candidate scores.",
        "healthy_pre_hand": healthy, "hand_interval": interval_quality(quality, HAND),
        "fast_section": fast,
        "rolling_healthy_100_image_rotation": rolling,
        "common_healthy_100_image_windows": common_windows(reference, candidate, mapping, quality, HEALTHY),
        "post_hand": post, "relocalized_video_frames_after_hand_arrival": relocalized,
        "upstream_intervals": {name: upstream_interval(reference, mapping, bounds)
                               for name, bounds in [("healthy_pre_hand", HEALTHY),
                                                    ("hand", HAND), ("clear_post_hand", POST)]},
        "first_relocalization_delay_from_clear_interval_ms":
            (relocalized[0] - POST[0]) * 10 if relocalized else None,
        "zero_relative_motion_violations": zero_motion_violations,
        "fresh_suffix_control": suffix_comparison,
        "same_recording_upstream_replay": same_video_comparison,
        "gates": gates, "passes": all(gates.values()),
        "thresholds": {"rotation_component_rmse_rad": args.max_rotation_rmse,
                       "min_healthy_acceptance": args.min_healthy_acceptance,
                       "min_post_hand_acceptance": args.min_post_acceptance,
                       "post_hand_fit_error_p95_multiplier": 2},
        "upstream_post_hand_is_not_ground_truth": True,
        "path_heading_note": "Pre-hand path/heading drift is reported, with no long-sequence parity claim. The source recording skips capture images; source timestamps are not video timestamps.",
        "candidate_summary": json.loads((candidate_dir / "summary.json").read_text()),
        "process_wall_seconds": run_wall,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as stream:
        json.dump(report, stream, indent=2)
        stream.write("\n")
    print(json.dumps({key: report[key] for key in ["healthy_pre_hand", "post_hand", "gates", "passes"]}, indent=2))
    if args.require_pass and not report["passes"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
