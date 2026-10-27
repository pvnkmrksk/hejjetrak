"""Audit the complete private VR3 motion structure without changing tracking.

Development analysis only; Python/matplotlib are not application dependencies.
The original frame map is mandatory. Every image/quality decision is retained;
single-capture derivatives exclude pairs spanning absent source images. Original
post-occlusion output is displayed as unreliable and never used as ground truth.
"""
import argparse
from collections import Counter
import csv
import json
import math
from pathlib import Path
import statistics

from benchmark_vr3 import FAST, HAND, HEALTHY, POST, STEM, dat_rows, sha256


def percentile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, max(0, math.ceil(len(ordered) * fraction) - 1))] if ordered else None


def correlation(x, y):
    if len(x) < 2:
        return None
    mx, my = statistics.mean(x), statistics.mean(y)
    xx = sum((value - mx) ** 2 for value in x)
    yy = sum((value - my) ** 2 for value in y)
    return sum((a - mx) * (b - my) for a, b in zip(x, y)) / math.sqrt(xx * yy) if xx > 0 and yy > 0 else None


def scalar_metrics(reference, candidate, unit):
    if not reference or len(reference) != len(candidate):
        return {"samples": 0, "unit": unit}
    residual = [b - a for a, b in zip(reference, candidate)]
    ref_rms = math.sqrt(statistics.mean(value * value for value in reference))
    cand_rms = math.sqrt(statistics.mean(value * value for value in candidate))
    power = sum(value * value for value in reference)
    scale = sum(a * b for a, b in zip(reference, candidate)) / power if power else None
    return {
        "samples": len(reference), "unit": unit,
        "reference_rms": ref_rms, "candidate_rms": cand_rms,
        "residual_rmse": math.sqrt(statistics.mean(value * value for value in residual)),
        "residual_mean_bias": statistics.mean(residual),
        "absolute_residual_median": statistics.median(abs(value) for value in residual),
        "absolute_residual_p95": percentile([abs(value) for value in residual], .95),
        "pearson_correlation": correlation(reference, candidate),
        "candidate_on_reference_zero_intercept_scale_diagnostic": scale,
        "candidate_to_reference_rms_ratio": cand_rms / ref_rms if ref_rms else None,
        "scale_note": "Descriptive diagnostic only; no scale or rotation was fitted to any plotted trajectory or measured derivative.",
    }


def vector_metrics(reference, candidate, unit):
    components = len(reference[0]) if reference else 0
    result = {"components": [scalar_metrics([v[j] for v in reference], [v[j] for v in candidate], unit)
                              for j in range(components)]}
    if reference:
        ref_norm = [math.sqrt(sum(x * x for x in row)) for row in reference]
        cand_norm = [math.sqrt(sum(x * x for x in row)) for row in candidate]
        result["norm"] = scalar_metrics(ref_norm, cand_norm, unit)
        result["vector_residual_rmse"] = math.sqrt(statistics.mean(
            sum((b - a) ** 2 for a, b in zip(left, right)) for left, right in zip(reference, candidate)))
    return result


def row_velocity(rows, i, dt, columns, difference=False):
    if not math.isfinite(dt) or dt <= 0:
        raise ValueError("Derivative requires a finite positive time interval")
    return [((rows[i][j] - rows[i - 1][j]) if difference else rows[i][j]) / dt for j in columns]


def angular_distance(a, b):
    return math.atan2(math.sin(a - b), math.cos(a - b))


def derive(reference, candidate, mapping, quality, indices):
    """Physical image-pair alignment; source clocks stay distinct and explicit."""
    good = [i for i in indices if i > 0 and mapping[i] == mapping[i - 1] + 1
            and quality[i]["status"] == "tracked" and quality[i - 1]["accepted"] == "true"]
    needed = set(good) | {i - 1 for i in good}
    aligned = {i: reference[mapping[i]] for i in needed}
    output = {}
    sources = {}
    for clock in ["own_output_clocks", "shared_reference_image_pair_clock"]:
        ref_dt = [(aligned[i][21] - aligned[i - 1][21]) / 1000 for i in good]
        cand_dt = ([(candidate[i][21] - candidate[i - 1][21]) / 1000 for i in good]
                   if clock == "own_output_clocks" else ref_dt)
        if any(dt <= 0 or not math.isfinite(dt) for dt in ref_dt + cand_dt):
            raise ValueError("Invalid aligned source time interval")
        measurements = {}
        for name, columns, difference, unit in [
            ("camera_angular_velocity", [1, 2, 3], False, "rad/s"),
            ("lab_angular_velocity", [5, 6, 7], False, "rad/s"),
            ("body_forward_side_velocity", [19, 20], True, "sphere_rad/s"),
            ("world_path_velocity", [14, 15], True, "sphere_rad/s"),
        ]:
            r = [row_velocity(aligned, i, dt, columns, difference) for i, dt in zip(good, ref_dt)]
            c = [row_velocity(candidate, i, dt, columns, difference) for i, dt in zip(good, cand_dt)]
            measurements[name] = vector_metrics(r, c, unit)
            sources[(clock, name)] = (good, r, c)
        r = [[angular_distance(aligned[i][16], aligned[i - 1][16]) / dt]
             for i, dt in zip(good, ref_dt)]
        c = [[angular_distance(candidate[i][16], candidate[i - 1][16]) / dt]
             for i, dt in zip(good, cand_dt)]
        measurements["heading_rate"] = vector_metrics(r, c, "rad/s")
        sources[(clock, "heading_rate")] = (good, r, c)
        r = [[aligned[i][18] / dt] for i, dt in zip(good, ref_dt)]
        c = [[candidate[i][18] / dt] for i, dt in zip(good, cand_dt)]
        measurements["body_speed"] = vector_metrics(r, c, "sphere_rad/s")
        sources[(clock, "body_speed")] = (good, r, c)
        output[clock] = measurements
    deltas = {}
    for label, columns in [("camera_rotation_increment", [1, 2, 3]), ("lab_rotation_increment", [5, 6, 7])]:
        deltas[label] = vector_metrics([[aligned[i][j] for j in columns] for i in good],
                                       [[candidate[i][j] for j in columns] for i in good], "rad/image_pair")
    # Absolute integrated coordinates can be compared at every mapped image,
    # including an image preceded by an unrecorded capture. Missing increments
    # remain absent from the Rust output and are not reconstructed or aligned away.
    integrated = {}
    for label, columns in [("heading_independent_body_movement", [19, 20]), ("world_path", [14, 15])]:
        selected = [i for i in indices if mapping[i] in reference]
        reference_values = [[reference[mapping[i]][j] for j in columns] for i in selected]
        candidate_values = [[candidate[i][j] for j in columns] for i in selected]
        measured = vector_metrics(reference_values, candidate_values, "sphere_rad")
        measured["endpoint_difference_components"] = [b - a for a, b in zip(reference_values[-1], candidate_values[-1])]
        measured["images"] = len(selected)
        measured["alignment"] = "Raw cumulative outputs; no translation, rotation, scale or later re-anchoring."
        integrated[label] = measured
    output.update({"measurement_pairs": len(good),
                   "source_gap_exclusions": [i for i in indices if i > 0 and mapping[i] != mapping[i - 1] + 1],
                   "unmeasured_pair_exclusions": [i for i in indices if i > 0 and (
                       quality[i]["status"] != "tracked" or quality[i - 1]["accepted"] != "true")],
                   "increments": deltas, "integrated_coordinates": integrated})
    return output, sources


def rolling_measurements(sources, bounds, length=100):
    result = {"window_images": length, "stride_images": 1, "metrics": {}}
    for name in ["camera_angular_velocity", "body_forward_side_velocity", "world_path_velocity", "heading_rate", "body_speed"]:
        ids, reference, candidate = sources[("shared_reference_image_pair_clock", name)]
        points = {i: (r, c) for i, r, c in zip(ids, reference, candidate)}
        components = len(reference[0])
        prefixes = [[0.0] for _ in range(7 + components)]
        for i in range(*bounds):
            if i in points:
                r, c = points[i]
                rn = math.sqrt(sum(v * v for v in r)); cn = math.sqrt(sum(v * v for v in c))
                values = [1.0, rn, cn, rn * rn, cn * cn, rn * cn, (cn - rn) ** 2]
                values.extend((b - a) ** 2 for a, b in zip(r, c))
            else:
                values = [0.0] * len(prefixes)
            for prefix, value in zip(prefixes, values):
                prefix.append(prefix[-1] + value)
        windows = []
        for start in range(bounds[0], bounds[1] - length + 1):
            begin = start - bounds[0]; end = begin + length
            sums = [prefix[end] - prefix[begin] for prefix in prefixes]
            if sums[0] != length:
                continue
            variance_r = max(0.0, sums[3] - sums[1] ** 2 / length)
            variance_c = max(0.0, sums[4] - sums[2] ** 2 / length)
            cov = sums[5] - sums[1] * sums[2] / length
            corr = cov / math.sqrt(variance_r * variance_c) if variance_r > 0 and variance_c > 0 else None
            windows.append({"video_frame_interval": [start, start + length],
                            "component_residual_rmse": [math.sqrt(max(0.0, value) / length) for value in sums[7:]],
                            "norm_correlation": corr,
                            "norm_residual_rmse": math.sqrt(max(0.0, sums[6]) / length)})
        errors = [v["norm_residual_rmse"] for v in windows]
        correlations = [v["norm_correlation"] for v in windows if v["norm_correlation"] is not None]
        result["metrics"][name] = {
            "windows": len(windows), "norm_residual_rmse_median": statistics.median(errors),
            "norm_residual_rmse_p95": percentile(errors, .95), "norm_residual_rmse_max": max(errors),
            "norm_correlation_median": statistics.median(correlations),
            "norm_correlation_min": min(correlations),
            "worst_residual_window": max(windows, key=lambda v: v["norm_residual_rmse"]),
            "nonoverlapping_window_examples": [v for v in windows if (v["video_frame_interval"][0] - bounds[0]) % length == 0],
        }
    return result


def compute_costs(matrix_path):
    matrix = json.loads(matrix_path.read_text())
    rows = []
    for group_set in matrix["matrices"]:
        for group in group_set["groups"]:
            detailed = json.loads((Path(group_set["evidence_directory"]) /
                                  f'{group["camera_count"]}-camera/report.json').read_text())
            for case in group["cases"]:
                # Full original report retains cumulative tracker counters and
                # per-poll post-acquisition worker EMAs omitted by the compact table.
                original = next(c for c in detailed["cases"] if c["mode"] == case["mode"])
                trackers = []
                for start, end in zip(original["before"], original["after"]):
                    count = end["tracking_samples"] - start["tracking_samples"]
                    if count > 0:
                        trackers.append((end["tracking_ms"] * end["tracking_samples"]
                                         - start["tracking_ms"] * start["tracking_samples"]) / count)
                workers = [session["processing_ms"] for sample in original["samples"] for session in sample["sessions"]]
                effective_codec = ("mjpg" if group["encoding_tuning"] == "mjpeg" else
                                   "raw" if group["encoding_tuning"] == "raw" else group["codec"])
                rows.append({"evidence_directory": group_set["evidence_directory"],
                             "camera_count": group["camera_count"], "configured_codec": group["codec"],
                             "encoding_tuning": group["encoding_tuning"],
                             "effective_codec": effective_codec,
                             "selection": case["mode"], "seconds": case["elapsed_seconds"],
                             "cpu_percent_one_logical_core": case["cpu_percent_one_core"],
                             "cpu_percent_of_eight_logical_capacity": case["cpu_percent_one_core"] / 8,
                             "processed_fps_per_camera": [v["processed_frames_per_second"] for v in case["progress"]],
                             "tracker_mean_ms_per_camera_during_interval": trackers,
                             "post_acquisition_worker_ema_samples_median_ms": statistics.median(workers),
                             "post_acquisition_worker_ema_samples_p95_ms": percentile(workers, .95),
                             "all_requested_video_complete": all(v["all_requested_video_complete"] for v in case["recordings"]),
                             "rss_median_mib": case["rss_median_bytes"] / 1024 ** 2})
    return {"host": matrix["host"], "cases": rows,
            "scope": "Existing serial2-4s measured native cases after1s warmup; complete sourcevideo buffers, data and wire checked separately. One-coreCPU100%=one logicalcore; total-capacity percent divides by8. Cumulative tracker means are differenced across interval endpoints. Worker values are median/p95 of polled EMA samples, not a per-frame latency distribution; camera Input::next/acquisition+adapterdecode and pacing are subtracted, while whole-processCPU includes all workers/GUI/transports. In overload modes most steady CPU follows explicit optional-video stop, not successful ongoing encoding. No long-run or physical ground-truth claim."}


def compact_summary(report):
    """Retain reproducible conclusions while leaving detailed windows in runs/."""
    scalar_keys = ["samples", "unit", "reference_rms", "candidate_rms", "residual_rmse",
                   "residual_mean_bias", "pearson_correlation", "candidate_to_reference_rms_ratio",
                   "candidate_on_reference_zero_intercept_scale_diagnostic"]

    def vector(value):
        return {"components": [{key: item[key] for key in scalar_keys if key in item}
                               for item in value["components"]],
                "vector_residual_rmse": value.get("vector_residual_rmse"),
                "norm": {key: value.get("norm", {}).get(key) for key in scalar_keys}}

    comparisons = {}
    for source, regions in report["comparisons"].items():
        comparisons[source] = {}
        for name, region in regions.items():
            comparisons[source][name] = {
                "measurement_pairs": region["measurement_pairs"],
                "source_gap_exclusions": region["source_gap_exclusions"],
                "unmeasured_pair_exclusions": region["unmeasured_pair_exclusions"],
                "increments": {key: vector(value) for key, value in region["increments"].items()},
                "shared_reference_image_pair_clock": {key: vector(value) for key, value in region["shared_reference_image_pair_clock"].items()},
                "own_output_clocks": {key: vector(value) for key, value in region["own_output_clocks"].items()} if name == "whole_clear_prehand" else None,
                "integrated_coordinates": {key: {
                    "images": value["images"], "vector_residual_rmse": value["vector_residual_rmse"],
                    "component_rmse": [v["residual_rmse"] for v in value["components"]],
                    "endpoint_difference_components": value["endpoint_difference_components"], "alignment": value["alignment"]}
                    for key, value in region["integrated_coordinates"].items()},
            }
    rolling = dict(report["rolling_shared_capture_clock"])
    rolling["metrics"] = {key: {k: v for k, v in value.items() if k != "nonoverlapping_window_examples"}
                          for key, value in rolling["metrics"].items()}
    post = {key: value for key, value in report["post_hand"].items() if key != "settled_suffix_vs_independently_initialized_atlas"}
    post["settled_suffix_camera_increments"] = vector(report["post_hand"]["settled_suffix_vs_independently_initialized_atlas"]["increments"]["camera_rotation_increment"])
    return {"fixture": report["fixture"], "video_frames": report["video_frames"], "files": report["files"],
            "clock_notes": report["clock_notes"], "comparisons": comparisons,
            "rolling_shared_capture_clock": rolling, "post_hand": post,
            "compute_costs": report["compute_costs"], "scope": report["scope"],
            "scale_note": "All scale/correlation figures are descriptive diagnostics; no fitted correction is applied to trajectories or velocities."}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--candidate-dir", type=Path, required=True)
    parser.add_argument("--suffix-dir", type=Path, required=True)
    parser.add_argument("--og-replay", type=Path, required=True)
    parser.add_argument("--matrix", type=Path, default=Path("docs/benchmarks/linux-recording-final-queue8-20261027.json"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--summary", type=Path, help="Optional new compact metrics JSON for documentation")
    args = parser.parse_args()
    if args.output.exists():
        parser.error("--output must be a new directory")
    if args.summary is not None and args.summary.exists():
        parser.error("--summary must be a new file")
    candidate = dat_rows(args.candidate_dir / "all-frames.dat")
    reference = {int(row[0]): row for row in dat_rows(args.reference_dir / f"fictrac-{STEM}.dat")}
    mapping = [int(value) for value in (args.reference_dir / f"fictrac-vidLogFrames-{STEM}.txt").read_text().splitlines()]
    with (args.candidate_dir / "quality.csv").open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    replay = dat_rows(args.og_replay)
    control = {int(row[0]): row for row in dat_rows(args.suffix_dir / "all-frames.dat")}
    if len(candidate) != 5157 or len(mapping) != 5157 or len(quality) != 5157 or len(replay) != 5157:
        raise ValueError("Expected complete5157-frame recording and aligned outputs")
    if any(row[0] != i or int(quality[i]["frame"]) != i or mapping[i] not in reference for i, row in enumerate(candidate)):
        raise ValueError("Candidate/source frame alignment failed")
    intervals = {"whole_clear_prehand": list(range(*HEALTHY)),
                 "before_fast": list(range(HEALTHY[0], FAST[0])),
                 "fast": list(range(*FAST)),
                 "after_fast_before_hand": list(range(FAST[1], HEALTHY[1])),
                 "outside_fast": list(range(HEALTHY[0], FAST[0])) + list(range(FAST[1], HEALTHY[1]))}
    comparisons = {}
    whole_sources = None
    for label, refs, frame_map in [("original_live", reference, mapping),
                                   ("original_same_avi", dict(enumerate(replay)), list(range(len(replay))))]:
        comparisons[label] = {}
        for name, indices in intervals.items():
            metrics, sources = derive(refs, candidate, frame_map, quality, indices)
            comparisons[label][name] = metrics
            if label == "original_live" and name == "whole_clear_prehand":
                whole_sources = sources
    original_dt = [reference[j][21] - reference[j - 1][21] for j in range(1, max(reference) + 1)]
    settled, _ = derive(control, candidate, list(range(len(candidate))), quality, list(range(4920, len(candidate))))
    post_ids = [i for i in range(*POST) if quality[i]["status"] == "tracked" and quality[i - 1]["accepted"] == "true"]
    pre_ids = [i for i in range(*HEALTHY) if quality[i]["status"] == "tracked"]
    def rates(ids):
        angular = [math.sqrt(sum(candidate[i][j] ** 2 for j in [1, 2, 3])) /
                   ((candidate[i][21] - candidate[i - 1][21]) / 1000) for i in ids]
        speed = [candidate[i][18] / ((candidate[i][21] - candidate[i - 1][21]) / 1000) for i in ids]
        errors = [candidate[i][4] for i in ids]
        return {"tracked_measurement_pairs": len(ids),
                "camera_angular_speed_rad_s": {"median": statistics.median(angular), "p95": percentile(angular, .95), "max": max(angular)},
                "body_speed_sphere_rad_s": {"median": statistics.median(speed), "p95": percentile(speed, .95), "max": max(speed)},
                "map_mse": {"median": statistics.median(errors), "p95": percentile(errors, .95), "max": max(errors)}}
    report = {"fixture": f"VR3{STEM}", "video_frames": len(candidate),
              "files": {"candidate": str(args.candidate_dir.resolve()),
                        "candidate_all_frames_sha256": sha256(args.candidate_dir / "all-frames.dat"),
                        "quality_sha256": sha256(args.candidate_dir / "quality.csv"),
                        "original_live_sha256": sha256(args.reference_dir / f"fictrac-{STEM}.dat"),
                        "source_frame_map_sha256": sha256(args.reference_dir / f"fictrac-vidLogFrames-{STEM}.txt"),
                        "og_replay": str(args.og_replay.resolve()), "og_replay_sha256": sha256(args.og_replay)},
              "clock_notes": {"original_capture_timestamp_column": 22, "candidate_encoded_video_clock_column": 22,
                              "original_single_capture_dt_ms": {"median": statistics.median(original_dt), "mean": statistics.mean(original_dt), "min": min(original_dt), "max": max(original_dt)},
                              "hardware_clock_limit": "The original CVSource uses the device timestamp when available and a host-clock fallback otherwise. This file is the available recorded source clock, not an independently verified shutter-time oracle.",
                              "candidate_dt_ms": 10,
                              "shared_clock": "Mapped image-pair capture timestamp differences from original column22 are applied to both unchanged motion estimates only for a clock-normalized diagnostic; no application output or trajectory is modified.",
                              "own_clocks": "Original live uses irregular native/host capture intervals (~8/12ms); encoded AVI candidate has10ms PTS. Comparing rates on own clocks includes those time-base differences.",
                              "source_gap_pairs": [i for i in range(1, len(mapping)) if mapping[i] - mapping[i - 1] != 1]},
              "comparisons": comparisons,
              "rolling_shared_capture_clock": rolling_measurements(whole_sources, HEALTHY),
              "post_hand": {"statuses": dict(Counter(q["status"] for q in quality[POST[0]:POST[1]])),
                            "healthy_prehand_ranges": rates(pre_ids), "recovered_ranges": rates(post_ids),
                            "settled_suffix_vs_independently_initialized_atlas": settled,
                            "interpretation": "Healthy-looking fit/speed ranges and same-decoder fresh-atlas agreement support resumed tracking. They do not provide independent physical ground truth or reconstruct occluded motion. Upstream post-hand output is not scored as truth."},
              "compute_costs": compute_costs(args.matrix),
              "scope": "All5157 image decisions retained. Main comparisons use the fixed clear pre-hand[2,4680) annotation; other ranges split/complement the predeclared fast[1690,2100). Pairwise derivatives exclude mapped missing-source-image pairs and unaccepted/relocalized measurements; no rejected zero is treated as measured stillness. Reported zero-intercept scale/correlation diagnostics are not applied as fitted trajectory corrections. No new scientific pass threshold replaces existing oracle gates."}
    args.output.mkdir(parents=True)
    (args.output / "metrics.json").write_text(json.dumps(report, indent=2, allow_nan=False))
    if args.summary is not None:
        args.summary.parent.mkdir(parents=True, exist_ok=True)
        args.summary.write_text(json.dumps(compact_summary(report), indent=2, allow_nan=False))
    print(json.dumps({"output": str(args.output.resolve()), "pairs": comparisons["original_live"]["whole_clear_prehand"]["measurement_pairs"],
                      "camera_correlation_shared_clock": [v["pearson_correlation"] for v in comparisons["original_live"]["whole_clear_prehand"]["shared_reference_image_pair_clock"]["camera_angular_velocity"]["components"]],
                      "body_speed_shared_clock": comparisons["original_live"]["whole_clear_prehand"]["shared_reference_image_pair_clock"]["body_speed"]["components"][0]}, indent=2))


if __name__ == "__main__":
    main()
