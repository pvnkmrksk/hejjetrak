"""Serial final release accuracy/performance evidence; never an application dependency.

Runs the required known-motion demo, public sample/unchanged compare and full
private VR3 replay/fresh suffix control. Standalone decoder passes are timed
separately; CLI tracking summary time is not relabeled as decoder time.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import resource
import subprocess
import sys
import time

from benchmark_vr3 import STEM, set_field


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def quiet():
    busy = []
    for process in Path("/proc").iterdir():
        if not process.name.isdigit():
            continue
        try:
            name = (process / "comm").read_text().strip()
        except OSError:
            continue
        if name in ("cargo", "rustc", "fictrac", "fictrac-desktop", "hejjetrac", "hejjetrak"):
            busy.append({"pid": int(process.name), "name": name})
    if busy:
        raise RuntimeError(f"serial timing requires no other build/native/tracking process: {busy}")


def run(command, output, label):
    quiet()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.perf_counter()
    completed = subprocess.run([str(v) for v in command], capture_output=True, text=True)
    elapsed = time.perf_counter() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
    (output / (label + ".stdout")).write_text(completed.stdout)
    (output / (label + ".stderr")).write_text(completed.stderr)
    if completed.returncode:
        raise RuntimeError(f"{label} exited {completed.returncode}; inspect saved logs")
    return {"command": [str(v) for v in command], "external_process_wall_seconds": elapsed,
            "child_cpu_seconds": cpu, "child_cpu_percent_one_core": 100 * cpu / elapsed,
            "cpu_boundary": "Completed child process plus waited descendants; includes all work inside this command, not only tracking. Percent of one core, no GPU/power measurement.",
            "returncode": completed.returncode}


def tracking_metrics(summary):
    frames = summary["frames"]
    internal = summary["wall_seconds"]
    tracking = frames / summary["tracking_fps"]
    return {"frames": frames, "internal_cli_pipeline_seconds": internal,
            "internal_cli_pipeline_fps": summary["end_to_end_fps"],
            "internal_tracker_process_seconds_derived": tracking,
            "internal_tracker_process_fps": summary["tracking_fps"],
            "other_pipeline_seconds_including_decode_threshold_output": internal - tracking,
            "pure_internal_decoder_seconds": None,
            "boundary": "Internal CLI wall starts at track entry: source/config/tracker setup, first-frame decode, processing, media finalization, map/path generation and numeric fsync included. Excludes process startup/argument parsing and final summary serialization/file promotion/stdout. Tracker time measures tracker.process only. Residual includes setup, prefetched decode/conversion/threshold/output; it is not decoder-only time."}


def decode(binary, source, frames, output, label):
    record = run([binary, "inspect", source, "--decode"], output, label)
    text = (output / (label + ".stdout")).read_text()
    match = re.search(r"decoded_frames=(\d+) last_timestamp_ms=([0-9.]+)", text)
    if not match or int(match[1]) != frames:
        raise RuntimeError(f"{label}: standalone decoder frame count differs from {frames}")
    record.update({"decoded_frames": frames, "last_timestamp_ms": float(match[2]),
                   "external_decode_pipeline_fps": frames / record["external_process_wall_seconds"],
                   "boundary": "Separate CLI inspect --decode process; luma frame decode/allocation, metadata output and process startup included. This is not the RGB+threshold decoder stage inside tracking and is not overlapped with tracking."})
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac"))
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--og-replay-dat", type=Path, required=True)
    parser.add_argument("--og-replay-run", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("this final fixture run is scoped to Linux")
    binary = args.binary.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    if args.report.exists():
        parser.error("--report must name a new file")
    quiet()
    root = Path(__file__).resolve().parent.parent
    cpu = next((line.split(":", 1)[1].strip() for line in Path("/proc/cpuinfo").read_text().splitlines()
                if line.startswith("model name")), "unavailable")
    report = {"status": "running", "platform": platform.platform(),
              "distribution": platform.freedesktop_os_release(), "logical_cpus": os.cpu_count(),
              "cpu": cpu, "rustc_verbose": subprocess.check_output(["rustc", "--version", "--verbose"], text=True),
              "cargo_version": subprocess.check_output(["cargo", "--version"], text=True).strip(),
              "binary": str(binary), "binary_sha256": digest(binary),
              "binary_version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
              "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "release_profile": "workspace release, thin LTO, codegen-units=1",
              "rustflags": os.environ.get("RUSTFLAGS"),
              "scope": "Serial release fixture runs on this host only; no native or Cargo process overlapped. Source fixtures and oracle tolerances unchanged. Fresh suffix control is separate from candidate timing. Original OG live/same-AVI timings have different boundaries and no comparable CPU metric."}
    try:
        demo = output / "demo120"
        report["demo120"] = run([binary, "demo", "--frames", "120", "--output", demo], output, "demo120")
        oracle = json.loads((demo / "oracle.json").read_text())
        summary = json.loads((demo / "tracking/summary.json").read_text())
        if not oracle["passed"] or summary["frames"] != 120:
            raise RuntimeError("known-motion demo failed its unchanged oracle")
        report["demo120"].update({"oracle": oracle, "summary": summary,
                                  "timing": tracking_metrics(summary), "input_sha256": digest(demo / "sphere.y4m"),
                                  "config_sha256": digest(demo / "config.json")})
        report["demo120"]["standalone_decode"] = decode(binary, demo / "sphere.y4m", 120, output, "demo120-decode")

        sample = root / "examples/sample.mp4"
        config = output / "sample-config.txt"
        original = root / "examples/benchmark/config.txt"
        text = original.read_text()
        for key, value in [("save_raw", "n"), ("save_debug", "n"), ("sock_port", "0"), ("zmq_port", "0"), ("com_port", "")]:
            text = set_field(text, key, value)
        config.write_text(text)
        tracked = output / "sample300"
        report["sample300"] = run([binary, "track", "--config", config, "--input", sample, "--output", tracked], output, "sample300")
        comparison = output / "sample-comparison.json"
        report["sample300"]["compare_process"] = run([binary, "compare", "--reference", root / "examples/benchmark/upstream.dat", "--candidate", tracked / "tracking.dat", "--report", comparison], output, "sample300-compare")
        summary = json.loads((tracked / "summary.json").read_text())
        result = json.loads(comparison.read_text())
        if summary["frames"] != 300 or summary["rejected"] != 0 or not result["passed"]:
            raise RuntimeError("public sample failed its unchanged comparison")
        report["sample300"].update({"summary": summary, "comparison": result, "timing": tracking_metrics(summary),
                                    "input_sha256": digest(sample), "original_config_sha256": digest(original),
                                    "effective_config_sha256": digest(config)})
        report["sample300"]["standalone_decode"] = decode(binary, sample, 300, output, "sample300-decode")

        vr3 = output / "vr3"
        vr3_comparison = output / "vr3-comparison.json"
        report["vr3"] = run([sys.executable, root / "scripts/benchmark_vr3.py", "--binary", binary,
                              "--reference-dir", args.reference_dir.resolve(), "--candidate-dir", vr3,
                              "--suffix-control-dir", output / "vr3-control", "--make-suffix-control",
                              "--upstream-replay-dat", args.og_replay_dat.resolve(),
                              "--output", vr3_comparison, "--require-pass"], output, "vr3-analysis-and-control")
        result = json.loads(vr3_comparison.read_text())
        summary = json.loads((vr3 / "summary.json").read_text())
        report["vr3"].update({"comparison": result, "summary": summary, "timing": tracking_metrics(summary),
                              "candidate_external_process_wall_seconds": result["process_wall_seconds"],
                              "wrapper_boundary": "Wrapper wall includes candidate, independent suffix-control second replay, and Python accuracy analysis; only candidate_external_process_wall_seconds times the single candidate CLI.",
                              "og_same_avi_dat_sha256": digest(args.og_replay_dat)})
        source = args.reference_dir / f"fictrac-raw-{STEM}.avi"
        report["vr3"]["standalone_decode"] = decode(binary, source, 5157, output, "vr3-decode")
        if args.og_replay_run:
            report["og_same_avi_external_run"] = json.loads(args.og_replay_run.read_text())
            report["og_same_avi_external_run"]["boundary"] = "Historical local OG process against exact same AVI, no output video/display; different implementation and timing boundaries, not a native camera CPU comparison."
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["failure"] = str(error)
        raise
    finally:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"status": report["status"], "binary_sha256": report["binary_sha256"],
                      "cases": {name: report[name]["timing"] for name in ("demo120", "sample300", "vr3")}}, indent=2))


if __name__ == "__main__":
    main()
