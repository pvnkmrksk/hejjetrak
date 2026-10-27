#!/usr/bin/env python3
"""Exercise daily native controls on real video and verify every saved artifact.

Development client only: the native JSONL bridge calls the same operations as
the visible toolbar and calibration panel. Original configs/videos stay intact.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import subprocess
import time

from test_native_rigs import NativeBridge, field


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    text = args.config.read_text()
    for key, value in [("src_fn", str(args.video.resolve())), ("save_raw", "n"),
                       ("save_debug", "n"), ("sock_port", "0"), ("zmq_port", "0"),
                       ("com_port", "")]:
        text = field(text, key, value)
    config = output / "config.txt"
    config.write_text(text)
    bridge = NativeBridge([str(args.binary.resolve()), "--config", str(config),
                           "--control-stdio", "--window-size", "520x480"], output)
    report = {"binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "video_sha256": hashlib.sha256(args.video.read_bytes()).hexdigest(),
              "rounds": [], "recordings": [], "passed": False}

    def status():
        return bridge.request("status", 1)

    def wait(predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            state = status()
            if state["error"] or state["command_error"]:
                raise RuntimeError(f"native control failed: {state}")
            if predicate(state):
                return state
            time.sleep(0.05)
        raise RuntimeError(f"native control did not complete: {state}")

    def advance(count):
        before = status()
        bridge.request("run", 1, frames=count)
        after = wait(lambda s: s["advance_remaining"] == 0
                     and s["frames"] >= before["frames"] + count)
        if after["frames"] != before["frames"] + count:
            raise RuntimeError("explicit advance lost its exact frame budget")
        return after

    def verify(directory, options):
        with next(directory.glob("*-all-frames.dat")).open(newline="") as stream:
            rows = list(csv.reader(stream))
        with next(directory.glob("*-quality.csv")).open(newline="") as stream:
            quality = list(csv.DictReader(stream))
        if not rows or len(rows) != len(quality):
            raise RuntimeError("saved numeric and quality rows do not align")
        if any(int(row[0]) != int(q["frame"]) for row, q in zip(rows, quality)):
            raise RuntimeError("saved quality identities differ from numeric rows")
        evidence = {"directory": str(directory), "rows": len(rows), "media": []}
        for kind in ("raw", "debug"):
            movies = list(directory.glob(f"*-media/{kind}.avi*"))
            if bool(movies) != options[kind] or len(movies) > 1:
                raise RuntimeError("recording selection differs from requested buttons")
            if not movies:
                continue
            movie = movies[0]
            if movie.suffix == ".partial":
                raise RuntimeError("lossless replay recording remained partial")
            probe = subprocess.run(["ffprobe", "-v", "error", "-count_frames",
                "-select_streams", "v:0", "-show_entries", "stream=width,height,codec_name,nb_read_frames",
                "-of", "json", str(movie)], capture_output=True, text=True, check=True)
            streams = json.loads(probe.stdout)["streams"]
            if probe.stderr.strip() or len(streams) != 1 or int(streams[0]["nb_read_frames"]) != len(rows):
                raise RuntimeError("independent full movie decode differs from saved decisions")
            if kind == "raw" and [streams[0]["width"], streams[0]["height"]] != [384, 288]:
                raise RuntimeError("source recording was spatially cropped or rescaled")
            evidence["media"].append({"kind": kind, "probe": streams[0],
                "sha256": hashlib.sha256(movie.read_bytes()).hexdigest()})
        return evidence

    try:
        discovery = bridge.request("discover")
        (output / "discovery.json").write_text(json.dumps(discovery, indent=2))
        wait(lambda s: s["paused"])
        canonical = bridge.request("config", 1)["text"]
        for iteration in range(args.rounds):
            if iteration:
                bridge.request("rewind", 1)
                wait(lambda s: s["paused"] and not s["ended"] and s["frames"] <= 1)
            advance(5)
            before = status()
            immutable = (before["frames"], before["revision"], before["last_row"])
            for enabled in (True, False, True):
                bridge.request("view", 1, settings=enabled, diagnostics=enabled,
                               full_image=enabled, overlays=False)
            bridge.request("view", 1, calibration=True)
            overview = status()
            if not overview["view"]["calibration"] or not overview.get("calibration_summary"):
                raise RuntimeError("one-click calibration overview/status is unavailable")
            for step in ("ball", "exclude", "lab"):
                bridge.request("view", 1, calibration_step=step, calibration_action="inspect")
                bridge.request("view", 1, calibration_action="keep")
            bridge.request("view", 1, calibration=True, calibration_step="ball",
                           calibration_action="redraw")
            bridge.request("view", 1, expect_error=True, calibration_action="continue")
            bridge.request("view", 1, calibration_action="cancel")
            bridge.request("view", 1, calibration=False)
            after = status()
            if (after["frames"], after["revision"], after["last_row"]) != immutable:
                raise RuntimeError("read-only calibration inspection/skip changed numerical state")
            if bridge.request("config", 1)["text"] != canonical:
                raise RuntimeError("daily inspection or empty-draft cancellation rewrote configuration")
            old_view = after["view"]
            bridge.request("view", 1, expect_error=True, settings=True, full_image="invalid")
            if status()["view"] != old_view:
                raise RuntimeError("invalid view request partially mutated the interface")
            saved = output / f"saved-{iteration}.txt"
            bridge.request("save_config", 1, path=str(saved))
            if saved.read_text() != canonical:
                raise RuntimeError("saved canonical configuration differs from inspected text")
            for kind, options in [("data", {"raw": False, "debug": False}),
                                  ("source", {"raw": True, "debug": False}),
                                  ("debug", {"raw": False, "debug": True}),
                                  ("both", {"raw": True, "debug": True})]:
                directory = output / f"record-{iteration}-{kind}"
                bridge.request("record_start", 1, directory=str(directory), record_options=options,
                               recording_tuning={"encoding": "raw"})
                wait(lambda s: s["recording"])
                advance(20)
                bridge.request("record_stop", 1)
                wait(lambda s: not s["recording"])
                report["recordings"].append((directory, options))
            reset_before = status()
            bridge.request("reset", 1)
            wait(lambda s: s["paused"])
            reset_after = advance(1)
            if reset_after["last_row"]["timestamp_ms"] <= reset_before["last_row"]["timestamp_ms"]:
                raise RuntimeError("reset rewound the source clock")
            if reset_after["last_row"]["sequence"] >= reset_before["last_row"]["sequence"]:
                raise RuntimeError("explicit reset did not reset the relative tracking sequence")
            report["rounds"].append({"iteration": iteration, "initial": before,
                                     "overview": overview, "final": reset_after})
        bridge.request("close", 1, discard=True)
        bridge.process.wait(timeout=20)
        if bridge.process.returncode:
            raise RuntimeError("native app did not close cleanly after button stress")
        # Independent media decoding happens after the app closes, outside any
        # live CPU/throughput measurement.
        report["recordings"] = [verify(directory, options)
                                 for directory, options in report["recordings"]]
        report["passed"] = True
    except Exception as error:
        report["failure"] = str(error)
        report["recordings"] = [str(item[0]) if isinstance(item, tuple) else item
                                for item in report["recordings"]]
        raise
    finally:
        (output / "report.json").write_text(json.dumps(report, indent=2))
        bridge.finish([1])
    print(json.dumps({"passed": True, "rounds": args.rounds,
                      "recordings": len(report["recordings"])}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
    parser.add_argument("--config", type=Path, default=Path("examples/benchmark/config.txt"))
    parser.add_argument("--video", type=Path, default=Path("examples/sample.mp4"))
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=5)
    args = parser.parse_args()
    if not 1 <= args.rounds <= 20:
        parser.error("--rounds must be 1..20")
    run(args)
