#!/usr/bin/env python3
"""Compare a complete native video replay with the same-build headless output.

The native session records all numeric/quality decisions while its real window
is resized and diagnostics toggled. No video images are skipped or transcoded.
Python is a development test client, never an application runtime dependency.
"""
import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import time

from test_native_rigs import NativeBridge, field


def numeric_rows(path):
    with path.open(newline="") as stream:
        rows = [[float(value) for value in values] for values in csv.reader(stream)]
    if any(len(row) != 25 or not all(math.isfinite(value) for value in row) for row in rows):
        raise ValueError(f"{path}: expected complete finite 25-column data")
    return rows


def row_values(row):
    return ([row["frame"]] + row["delta_camera"] + [row["error"]]
            + row["delta_lab"] + row["absolute_camera"] + row["absolute_lab"]
            + row["position"] + [row["heading"], row["direction"], row["speed"]]
            + row["integrated"] + [row["timestamp_ms"], row["sequence"], row["delta_ms"], row["timestamp_ms"]])


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    config = args.config.read_text()
    for key, value in [("src_fn", str(args.video.resolve())), ("save_raw", "n"),
                       ("save_debug", "n"), ("sock_port", "0"), ("zmq_port", "0"),
                       ("com_port", "")]:
        config = field(config, key, value)
    config_path = output / "config.txt"
    config_path.write_text(config)
    reference = numeric_rows(args.headless / "all-frames.dat")
    with (args.headless / "quality.csv").open(newline="") as stream:
        reference_quality = list(csv.DictReader(stream))
    expected = len(reference)
    if expected < 2 or len(reference_quality) != expected:
        raise ValueError("headless reference needs all-frame numeric and quality evidence")
    bridge = NativeBridge([
        str(args.binary.resolve()), "--config", str(config_path), "--control-stdio",
        "--window-size", "520x480", "--screenshot", str(output / "native-window.png"),
        "--screenshot-after", "3",
    ], output)
    report = {"video": str(args.video.resolve()), "video_sha256": digest(args.video),
              "working_directory": str(output),
              "binary": str(args.binary.resolve()), "binary_sha256": digest(args.binary),
              "headless_reference": str(args.headless.resolve()), "expected_frames": expected,
              "paced": not args.unpaced, "resizes": []}

    def status_until(predicate, timeout=120):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            status = bridge.request("status", 1)
            if status["error"] or status["command_error"]:
                raise RuntimeError(f"native replay failed: {status}")
            if predicate(status):
                return status
            time.sleep(0.05)
        raise RuntimeError(f"native replay did not converge: {status}")

    try:
        initial = status_until(lambda s: s["paused"])
        report["initial"] = initial
        if initial["frames"] > 1:
            raise RuntimeError("startup advanced more than its retained initialization row before pause")
        bridge.request("record_start", 1, directory=str(output / "recording"),
                       record_options={"raw": False, "debug": False})
        status_until(lambda s: s["recording"])
        bridge.request("realtime", 1, enabled=not args.unpaced)
        bridge.request("start", 1)
        started = time.monotonic()
        # Includes portrait/wide/tiny transitions during healthy fast motion,
        # then ordinary diagnostics and post-occlusion recovery playback.
        schedule = [(100, 640, 480, False), (500, 180, 140, False),
                    (1000, 96, 96, False), (1500, 520, 480, True),
                    (1650, 1000, 250, True), (1800, 96, 96, True),
                    (2100, 640, 480, False), (3000, 180, 140, True),
                    (4000, 520, 480, False), (4800, 640, 480, True)]
        for target, width, height, diagnostics in schedule:
            if target >= expected:
                continue
            before = status_until(lambda s: s["frames"] >= target or s["ended"])
            if before["ended"]:
                raise RuntimeError("native video ended before the declared source frame count")
            bridge.request("view", 1, window_size=[width, height], diagnostics=diagnostics)
            resized = status_until(lambda s: s["window_inner_size"] is not None
                                   and abs(s["window_inner_size"][0] - width) <= 40
                                   and abs(s["window_inner_size"][1] - height) <= 40)
            report["resizes"].append({"at_frame": before["frames"], "requested": [width, height],
                                      "diagnostics": diagnostics, "status": resized})
            if 1650 <= target <= 1800:
                snapshot_path = output / f"fast-motion-{target}"
                bridge.request("snapshot", 1, directory=str(snapshot_path))
        final = status_until(lambda s: s["ended"], timeout=max(120, expected / 50))
        report["process_observation_seconds"] = time.monotonic() - started
        report["final"] = final
        if final["frames"] != expected or final["recording"]:
            raise RuntimeError(f"native replay did not retain/finalize all {expected} frames")
        native = numeric_rows(output / "recording" / "epoch-0000-all-frames.dat")
        with (output / "recording" / "epoch-0000-quality.csv").open(newline="") as stream:
            quality = list(csv.DictReader(stream))
        if initial["frames"] == 1:
            row = initial["last_row"]
            native.insert(0, row_values(row))
            quality.insert(0, {"frame": str(row["frame"]), "timestamp_ms": str(row["timestamp_ms"]),
                               "accepted": str(row["accepted"]).lower(), "status": row["status"],
                               "fit_error": str(row["error"])})
        if len(native) != expected or len(quality) != expected:
            raise RuntimeError("native recording has missing or duplicated rows")
        if any(row[0] != i for i, row in enumerate(native)):
            raise RuntimeError("native frame IDs do not cover the complete source")
        merged = output / "all-frames.dat"
        merged.write_text("".join(", ".join(f"{value:.12f}" for value in row) + "\n" for row in native))
        max_columns = [max(abs(a[j] - b[j]) for a, b in zip(native, reference)) for j in range(25)]
        rmse = [math.sqrt(sum((a[j] - b[j]) ** 2 for a, b in zip(native, reference)) / expected)
                for j in (1, 2, 3)]
        path_rmse = math.sqrt(sum(sum((a[j] - b[j]) ** 2 for j in (14, 15))
                                  for a, b in zip(native, reference)) / expected)
        heading_rmse = math.sqrt(sum(math.atan2(math.sin(a[16] - b[16]), math.cos(a[16] - b[16])) ** 2
                                     for a, b in zip(native, reference)) / expected)
        decisions_match = all(a["accepted"] == b["accepted"] and a["status"] == b["status"]
                              and int(a["frame"]) == int(b["frame"])
                              for a, b in zip(quality, reference_quality))
        report["comparison"] = {"frames": len(native), "max_absolute_difference_by_column": max_columns,
                                "rotation_component_rmse_rad": rmse,
                                "trajectory_point_rmse_sphere_rad": path_rmse,
                                "wrapped_heading_rmse_rad": heading_rmse,
                                "all_quality_decisions_match": decisions_match,
                                "numeric_epsilon": 1e-9}
        report["passed"] = decisions_match and all(value <= 1e-9 for value in max_columns)
        if not report["passed"]:
            raise RuntimeError("native tracking differs from same-build headless output; see report.json")
        bridge.request("close", 1, discard=True)
        bridge.process.wait(timeout=15)
        if bridge.process.returncode != 0:
            raise RuntimeError(f"native app exit code {bridge.process.returncode}")
    except Exception as error:
        report["passed"] = False
        report["failure"] = str(error)
        raise
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        bridge.finish([1])
    print(json.dumps({"passed": True, "report": str(output / "report.json"),
                      "comparison": report["comparison"]}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
    parser.add_argument("--video", type=Path, required=True)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--headless", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--unpaced", action="store_true")
    run(parser.parse_args())
