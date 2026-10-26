#!/usr/bin/env python3
"""Verify the actual native New/Open-four path starting from VR3.

Both conventional VR3/config.txt and flat VR3.txt layouts are exercised. The
saved physical calibrations are copied with media/wire outputs disabled; the
original files are never changed. This test opens all four physical cameras.
"""
import argparse
import hashlib
import json
from pathlib import Path
import time

from test_native_rigs import NativeBridge, field


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    cases = []
    for layout in ["folders", "flat"]:
        case = output / layout
        case.mkdir()
        for number in range(1, 5):
            text = (args.rig_directory / f"VR{number}.txt").read_text()
            for key, value in [("save_raw", "n"), ("save_debug", "n"),
                               ("sock_port", "0"), ("zmq_port", "0")]:
                text = field(text, key, value)
            path = (case / "configs" / f"VR{number}" / "config.txt" if layout == "folders"
                    else case / "configs" / f"VR{number}.txt")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        config = (case / "configs" / "VR3" / "config.txt" if layout == "folders"
                  else case / "configs" / "VR3.txt")
        bridge = NativeBridge([
            str(args.binary.resolve()), "--config", str(config), "--control-stdio",
            "--grid", "2x2", "--window-size", "480x360",
            "--screenshot", str(case / "native.png"), "--screenshot-after", "3",
        ], case)
        record = {"layout": layout}
        ids = [1]
        try:
            bridge.request("start", 1)
            current = 1
            for _ in range(3):
                # This dispatches Desktop::spawn_from, the same function used
                # by the rig manager's Open four rigs button.
                response = bridge.request("new_instance", current)
                current = response["status"]["session"]
                ids.append(current)
            states = bridge.wait(lambda states: len(states) == 4 and all(
                s["frames"] >= 5 and s["camera"] is not None for s in states
            ))
            sources = [s["source"] for s in states]
            if sources != ["4", "6", "0", "2"]:
                raise RuntimeError(f"VR3 did not cycle through the four real rigs: {sources}")
            expected_names = ["VR3", "VR4", "VR1", "VR2"]
            if any(not s["window_title"].startswith(name + " ·")
                   for s, name in zip(states, expected_names)):
                raise RuntimeError(f"native rig names differ from the selected saved configs: {states}")
            if not all(s["native_window"] and s["last_row"] is not None for s in states):
                raise RuntimeError("a native rig has no image/tracking state")
            snapshots = []
            for state in states:
                directory = case / f"session-{state['session']}-image"
                bridge.request("snapshot", state["session"], directory=str(directory))
                if not (directory / "input.png").is_file():
                    raise RuntimeError(f"native rig has no retained source pixels: {state}")
                snapshots.append(str(directory))
            before = {s["session"]: s["frames"] for s in states}
            deadline = time.monotonic() + 12
            while not (case / "native.png").exists():
                if time.monotonic() >= deadline:
                    raise RuntimeError("native VR3-starting grid screenshot was not delivered")
                bridge.request("sessions")
                time.sleep(0.1)
            final = bridge.wait(lambda states: all(s["frames"] > before[s["session"]] for s in states))
            record.update({"passed": True, "sources": sources, "sessions": final,
                           "computed_source_and_debug_images": snapshots})
            for identity in reversed(ids):
                bridge.request("close", identity, discard=True)
            bridge.process.wait(timeout=15)
            if bridge.process.returncode != 0:
                raise RuntimeError(f"native grid exited {bridge.process.returncode}")
        except Exception as error:
            record.update({"passed": False, "failure": str(error)})
            raise
        finally:
            cases.append(record)
            (case / "report.json").write_text(json.dumps(record, indent=2) + "\n")
            bridge.finish(ids)
            (output / "report.json").write_text(json.dumps({
                "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                "cases": cases, "passed": all(c["passed"] for c in cases),
            }, indent=2) + "\n")
    print(json.dumps({"passed": True, "cases": [{"layout": c["layout"], "sources": c["sources"]}
                                                   for c in cases]}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
    parser.add_argument("--rig-directory", type=Path, default=Path("."))
    parser.add_argument("--output", type=Path, required=True)
    run(parser.parse_args())
