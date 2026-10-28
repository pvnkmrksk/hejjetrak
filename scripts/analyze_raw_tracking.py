"""Compare same-video Rust and original FicTrac replays without ground truth.

Development analysis only; Python is not an application dependency. Frame IDs
are joined exactly, with an optional explicit integer offset. No lag, scale,
axis rotation or trajectory fit is chosen to improve agreement. Original
FicTrac data has no validity flag and is a baseline, not a truth measurement.

  python3 scripts/analyze_raw_tracking.py --candidate-dir runs/candidate \
      --reference-dat runs/original/fictrac.dat --output runs/comparison.json
"""

import argparse
from collections import Counter
import csv
import hashlib
import json
import math
from pathlib import Path
import statistics


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def dat_rows(path):
    rows = {}
    with path.open(newline="") as stream:
        for line, fields in enumerate(csv.reader(stream), 1):
            values = [float(value) for value in fields]
            if len(values) != 25 or not all(math.isfinite(value) for value in values):
                raise ValueError(f"{path}:{line}: expected 25 finite numeric columns")
            frame = int(values[0])
            if frame != values[0] or frame < 0 or (rows and frame <= next(reversed(rows))):
                raise ValueError(f"{path}:{line}: frame IDs must be increasing nonnegative integers")
            rows[frame] = values
    if len(rows) < 2:
        raise ValueError(f"{path}: at least two rows are required")
    return rows


def quality_rows(path, candidate):
    quality = {}
    valid = {"tracked": True, "initializing": True, "relocalized": True,
             "rejected": False, "recovering": False}
    with path.open(newline="") as stream:
        for line, row in enumerate(csv.DictReader(stream), 2):
            frame = int(row["frame"])
            status = row["status"]
            if frame in quality or frame not in candidate or status not in valid:
                raise ValueError(f"{path}:{line}: duplicate/missing frame or unknown status")
            if row["accepted"] != str(valid[status]).lower():
                raise ValueError(f"{path}:{line}: acceptance conflicts with status")
            timestamp, error = float(row["timestamp_ms"]), float(row["fit_error"])
            if (not math.isfinite(timestamp) or not math.isfinite(error)
                    or abs(timestamp - candidate[frame][21]) > 1e-5
                    or abs(error - candidate[frame][4]) > 1e-5):
                raise ValueError(f"{path}:{line}: quality does not match numeric evidence")
            quality[frame] = {"status": status, "accepted": valid[status]}
    if set(quality) != set(candidate):
        raise ValueError(f"{path}: quality and all-frame numeric IDs differ")
    return quality


def distribution(values):
    ordered = sorted(values)
    if not ordered:
        return {"samples": 0}
    result = {"samples": len(ordered), "min": ordered[0], "max": ordered[-1],
              "mean": statistics.mean(ordered)}
    for label, fraction in [("p50", .5), ("p90", .9), ("p95", .95), ("p99", .99)]:
        result[label] = ordered[max(0, math.ceil(fraction * len(ordered)) - 1)]
    return result


def correlation(left, right):
    if len(left) < 2:
        return None
    ml, mr = statistics.mean(left), statistics.mean(right)
    ll = sum((value - ml) ** 2 for value in left)
    rr = sum((value - mr) ** 2 for value in right)
    return (sum((a - ml) * (b - mr) for a, b in zip(left, right)) / math.sqrt(ll * rr)
            if ll > 0 and rr > 0 else None)


def scalar_comparison(left, right):
    if not left:
        return {"samples": 0}
    return {"samples": len(left),
            "difference_rmse": math.sqrt(statistics.mean((b - a) ** 2 for a, b in zip(left, right))),
            "difference_mean": statistics.mean(b - a for a, b in zip(left, right)),
            "pearson_correlation": correlation(left, right),
            "reference": distribution(left), "candidate": distribution(right)}


def ranges(frames):
    result = []
    for frame in frames:
        if result and frame == result[-1][1]:
            result[-1][1] += 1
        else:
            result.append([frame, frame + 1])
    return result


def rotation_metrics(reference, candidate, frames, offset):
    return [scalar_comparison([reference[i + offset][axis] for i in frames],
                              [candidate[i][axis] for i in frames]) for axis in (1, 2, 3)]


def wrap(angle):
    return math.atan2(math.sin(angle), math.cos(angle))


def trajectory(reference, candidate, frames, offset):
    if not frames:
        return {"samples": 0}
    ref0, cand0 = reference[frames[0] + offset], candidate[frames[0]]
    angle = ref0[16] - cand0[16]
    cosine, sine = math.cos(angle), math.sin(angle)
    raw, aligned, heading, relative_heading = [], [], [], []
    for i in frames:
        ref, cand = reference[i + offset], candidate[i]
        raw.append((cand[14] - ref[14]) ** 2 + (cand[15] - ref[15]) ** 2)
        cx, cy = cand[14] - cand0[14], cand[15] - cand0[15]
        rx, ry = ref[14] - ref0[14], ref[15] - ref0[15]
        aligned.append((cosine * cx - sine * cy - rx) ** 2
                       + (sine * cx + cosine * cy - ry) ** 2)
        heading.append(wrap(cand[16] - ref[16]) ** 2)
        relative_heading.append(wrap((cand[16] - cand0[16]) - (ref[16] - ref0[16])) ** 2)
    return {"samples": len(frames), "raw_path_rmse_sphere_rad": math.sqrt(statistics.mean(raw)),
            "raw_endpoint_error_sphere_rad": math.sqrt(raw[-1]),
            "raw_wrapped_heading_rmse_rad": math.sqrt(statistics.mean(heading)),
            "initial_pose_aligned_path_rmse_sphere_rad": math.sqrt(statistics.mean(aligned)),
            "initial_pose_aligned_endpoint_error_sphere_rad": math.sqrt(aligned[-1]),
            "relative_wrapped_heading_rmse_rad": math.sqrt(statistics.mean(relative_heading)),
            "alignment": "Only initial position and heading; no later fitting, scaling or gap reconstruction."}


def motion_structure(reference, candidate, frames, measured, offset):
    velocities = {"reference": {}, "candidate": {}}
    for name, rows, shift, selected in [("reference", reference, offset, frames),
                                        ("candidate", candidate, 0, measured)]:
        for i in selected:
            dt = (candidate[i][21] - candidate[i - 1][21]) / 1000
            velocities[name][i] = [rows[i + shift][axis] / dt for axis in (1, 2, 3)]
    output = {}
    for name, velocity in velocities.items():
        accelerations = []
        for i in velocity:
            if i - 1 in velocity:
                # Velocity samples represent image intervals; their centers are
                # separated by the average duration of the adjacent intervals.
                dt = (candidate[i][21] - candidate[i - 2][21]) / 2000
                accelerations.append(math.sqrt(sum((velocity[i][axis] - velocity[i - 1][axis]) ** 2
                                                   for axis in range(3))) / dt)
        output[name] = {
            "delta_norm_rad": distribution([math.sqrt(sum((v * (candidate[i][21] - candidate[i - 1][21]) / 1000) ** 2
                                                          for v in vector)) for i, vector in velocity.items()]),
            "angular_speed_rad_per_second": distribution([math.sqrt(sum(v * v for v in vector))
                                                           for vector in velocity.values()]),
            "angular_acceleration_norm_rad_per_second_squared": distribution(accelerations)}
    common = [i for i in measured if i in velocities["reference"]]
    output["common_measured_angular_speed_comparison"] = scalar_comparison(
        [math.sqrt(sum(v * v for v in velocities["reference"][i])) for i in common],
        [math.sqrt(sum(v * v for v in velocities["candidate"][i])) for i in common])
    output["clock"] = "Both derivatives use the validated candidate video presentation intervals. Candidate excludes every unmeasured delta; acceleration never spans a missing interval."
    return output


def analyze(candidate_dir, reference_path, offset=0, window=100):
    candidate_path, quality_path = candidate_dir / "all-frames.dat", candidate_dir / "quality.csv"
    candidate, reference = dat_rows(candidate_path), dat_rows(reference_path)
    quality = quality_rows(quality_path, candidate)
    ids = list(candidate)
    if any(b != a + 1 for a, b in zip(ids, ids[1:])):
        raise ValueError("Candidate all-frame IDs are not contiguous")
    if any(candidate[b][21] <= candidate[a][21] for a, b in zip(ids, ids[1:])):
        raise ValueError("Candidate video timestamps do not strictly increase")
    common = [i for i in candidate if i + offset in reference]
    pairs = [i for i in common if i - 1 in candidate and i - 1 + offset in reference]
    if not pairs:
        raise ValueError("No common consecutive image pairs at the specified frame offset")
    measured = [i for i in pairs if quality[i]["status"] == "tracked" and quality[i - 1]["accepted"]]
    rejected = [i for i in candidate if not quality[i]["accepted"]]
    missing_motion = [i for i in candidate if quality[i]["status"] != "tracked"]
    rejected_ranges = ranges(rejected)
    clocks = [(candidate[i][21] - candidate[i - 1][21],
               reference[i + offset][21] - reference[i - 1 + offset][21]) for i in pairs]
    # Upstream's first output timestamp/interval is known to be anomalous. Keep
    # it in raw evidence, exclude only the first pair from clock diagnostics.
    clock_diagnostic = clocks[1:]
    windows = []
    pair_set = set(pairs)
    for begin in range(ids[0] + 1, ids[-1] - window + 2):
        selected = list(range(begin, begin + window))
        if not all(i in pair_set for i in selected):
            continue
        metrics = rotation_metrics(reference, candidate, selected, offset)
        windows.append({"candidate_frame_interval": [begin, begin + window],
                        "measured_relative_rows": sum(quality[i]["status"] == "tracked" for i in selected),
                        "component_difference_rmse_rad_all_decisions": [axis["difference_rmse"] for axis in metrics]})
    worst = [max(windows, key=lambda row: row["component_difference_rmse_rad_all_decisions"][axis])
             for axis in range(3)] if windows else []
    return {
        "scope": "Same-video original FicTrac comparison; neither output is physical ground truth. Agreement, acceptance and low image error do not establish motion accuracy.",
        "inputs": {"candidate_all_frames": str(candidate_path.resolve()), "candidate_sha256": sha256(candidate_path),
                   "quality": str(quality_path.resolve()), "quality_sha256": sha256(quality_path),
                   "reference": str(reference_path.resolve()), "reference_sha256": sha256(reference_path)},
        "alignment": {"method": "reference_frame = candidate_frame + explicit offset; no automatic lag selection",
                      "frame_offset": offset, "candidate_frame_range_inclusive": [ids[0], ids[-1]],
                      "reference_frame_range_inclusive": [min(reference), max(reference)],
                      "common_rows": len(common), "common_consecutive_image_pairs": len(pairs),
                      "candidate_rows_without_reference": ranges([i for i in candidate if i + offset not in reference]),
                      "reference_missing_ID_ranges": ranges([i for i in range(min(reference), max(reference) + 1) if i not in reference]),
                      "candidate_timestamp_interval_ms": distribution([a for a, _ in clocks]),
                      "reference_timestamp_interval_ms_excluding_first_pair": distribution([b for _, b in clock_diagnostic]),
                      "timestamp_interval_difference_ms_excluding_first_pair": distribution([a - b for a, b in clock_diagnostic]),
                      "reference_clock_mismatches": [{"candidate_frame": i, "reference_interval_ms": b,
                                                       "candidate_interval_ms": a}
                                                      for i, (a, b) in zip(pairs, clocks) if abs(a - b) > .001],
                      "clock_note": "Frame identity is asserted by the same-video replay provenance, not inferred from wall-clock timestamps. All reference-clock mismatches, including initial or terminal clock switches, are retained explicitly. Shared video intervals drive derivatives."},
        "counts": {"candidate": len(candidate), "reference": len(reference),
                   "candidate_statuses": dict(Counter(row["status"] for row in quality.values())),
                   "accepted": len(candidate) - len(rejected), "measured_relative": sum(row["status"] == "tracked" for row in quality.values()),
                   "unaccepted": len(rejected), "missing_relative_measurement_including_initialization": len(missing_motion)},
        "gaps": {"unaccepted_frame_intervals": rejected_ranges,
                 "longest_unaccepted_run_frames": max((end - begin for begin, end in rejected_ranges), default=0),
                 "all_unmeasured_frame_intervals_including_initialization_and_relocalization": ranges(missing_motion),
                 "semantics": "Frozen all-frame deltas/paths are missing measurements. Relocalization publishes no hidden motion. Original .dat has no validity flag."},
        "rotation_comparison": {"all_decisions_including_frozen_missing_rows": rotation_metrics(reference, candidate, pairs, offset),
                                "common_measured_relative_only_diagnostic": rotation_metrics(reference, candidate, measured, offset)},
        "trajectory_all_common_rows": trajectory(reference, candidate, common, offset),
        "rolling_comparison": {"window_frames": window, "overlapping_windows": len(windows),
                               "worst_window_per_component": worst,
                               "component_rmse_distributions_rad": [distribution([row["component_difference_rmse_rad_all_decisions"][axis] for row in windows]) for axis in range(3)],
                               "note": "Every available overlapping window includes all candidate decisions, including frozen missing-motion rows."},
        "motion_structure": motion_structure(reference, candidate, pairs, measured, offset),
        "fit_error": {"all_decisions": distribution([row[4] for row in candidate.values()]),
                      "measured_relative": distribution([candidate[i][4] for i in candidate if quality[i]["status"] == "tracked"]),
                      "unaccepted": distribution([candidate[i][4] for i in rejected])},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--reference-dat", type=Path, required=True)
    parser.add_argument("--frame-offset", type=int, default=0)
    parser.add_argument("--window", type=int, default=100)
    parser.add_argument("--input-video", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.window < 2:
        parser.error("--window must be at least two")
    report = analyze(args.candidate_dir, args.reference_dat, args.frame_offset, args.window)
    if args.input_video:
        report["inputs"]["video"] = str(args.input_video.resolve())
        report["inputs"]["video_sha256"] = sha256(args.input_video)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x") as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
        stream.write("\n")
    print(json.dumps({"output": str(args.output), "counts": report["counts"],
                      "longest_unaccepted_run_frames": report["gaps"]["longest_unaccepted_run_frames"],
                      "rotation_component_rmse_rad": [axis["difference_rmse"] for axis in report["rotation_comparison"]["all_decisions_including_frozen_missing_rows"]]}))


if __name__ == "__main__":
    main()
