#!/usr/bin/env python3
"""Exercise real native sessions through their asynchronous JSONL bridge.

Run with the requested physical rig configs and a new evidence directory:
  python3 scripts/test_native_rigs.py --configs VR1.txt VR2.txt VR3.txt VR4.txt \
      --output runs/native-four

The configs are copied; automatic recording is disabled in the copies. Camera
geometry, source IDs, and output ports are preserved. This opens native windows
and requires access to the display and cameras. It is not a headless GUI test.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import re
import subprocess
import threading
import time


def field(text, name, value):
    pattern = rf"(?m)^(\s*{re.escape(name)}\s*:\s*)[^\r\n#]*"
    if re.search(pattern, text):
        return re.sub(pattern, lambda match: match[1] + value, text)
    return text.rstrip() + f"\n{name} : {value}\n"


def frame_progress(before, after, elapsed):
    initial = {s["session"]: s for s in before}
    return [{"session": s["session"], "source": s["source_label"],
             "processed_frame_delta": s["frames"] - initial[s["session"]]["frames"],
             "processed_frames_per_second": (s["frames"] - initial[s["session"]]["frames"]) / elapsed,
             "advertised_source_fps": s["source_fps"]}
            for s in after]


class NativeBridge:
    def __init__(self, command, output):
        self.stderr = (output / "stderr.log").open("w")
        self.transcript = (output / "bridge.jsonl").open("w")
        self.process = subprocess.Popen(
            command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.stderr, text=True, bufsize=1, cwd=output.resolve(),
        )
        self.responses = queue.Queue()
        self.counter = 0

        def read():
            for line in self.process.stdout:
                try:
                    self.responses.put(json.loads(line))
                except json.JSONDecodeError:
                    self.responses.put({"invalid_stdout": line})
            self.responses.put(None)

        threading.Thread(target=read, daemon=True).start()

    def request(self, command, session=None, *, expect_error=False, **args):
        self.counter += 1
        request = {"id": self.counter, "command": command, "args": args}
        if session is not None:
            request["session"] = session
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        try:
            response = self.responses.get(timeout=30)
        except queue.Empty as error:
            self.transcript.write(json.dumps({"request": request, "response_timeout_seconds": 30,
                                              "process_exit_code": self.process.poll()}) + "\n")
            self.transcript.flush()
            raise RuntimeError(f"native request {self.counter} ({command}) timed out after 30s; "
                               f"process exit code: {self.process.poll()}") from error
        self.transcript.write(json.dumps({"request": request, "response": response}) + "\n")
        self.transcript.flush()
        if response is None:
            raise RuntimeError(f"native process exited ({self.process.poll()}); see stderr.log")
        if response.get("id") != self.counter:
            raise RuntimeError(f"native response ID mismatch: {response}")
        if expect_error:
            if response.get("ok"):
                raise RuntimeError(f"native request unexpectedly succeeded: {response}")
            return response["error"]
        if not response.get("ok"):
            raise RuntimeError(f"native request failed: {response}")
        return response["result"]

    def wait(self, predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            sessions = self.request("sessions")
            failures = [s for s in sessions if s["ended"] or s["error"] or s["command_error"]]
            if failures:
                raise RuntimeError(f"native session failed: {failures}")
            if predicate(sessions):
                return sessions
            time.sleep(0.1)
        raise RuntimeError(f"native state did not converge within {timeout}s: {sessions}")

    def finish(self, identities=()):
        if self.process.poll() is None:
            # Ask only this harness's native sessions to close before falling
            # back to terminating an unresponsive test process. This releases
            # capture streams cleanly even when a different source failed.
            try:
                for identity in reversed(identities):
                    self.counter += 1
                    self.process.stdin.write(json.dumps({
                        "id": self.counter, "command": "close", "session": identity,
                        "args": {"discard": True},
                    }) + "\n")
                self.process.stdin.flush()
            except (BrokenPipeError, OSError):
                pass
            try:
                self.process.stdin.close()
            except (BrokenPipeError, OSError):
                pass
            try:
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                self.process.wait(timeout=10)
        self.stderr.close()
        self.transcript.close()


def run(args):
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    copied = []
    for index, original in enumerate(args.configs, 1):
        original = original.resolve()
        name = original.stem if original.stem != "config" else original.parent.name
        if not re.fullmatch(r"(?:VR|BR)\d+", name):
            name = f"VR{index}"
        config = output / "configs" / f"VR{index}" / "config.txt"
        config.parent.mkdir(parents=True)
        text = original.read_text()
        for key, value in [("save_raw", "n"), ("save_debug", "n"), ("rust_instance_name", name)]:
            text = field(text, key, value)
        if args.zmq_base_port is not None:
            for key, value in [("zmq_port", str(args.zmq_base_port + index - 1)),
                               ("zmq_host", "127.0.0.1"), ("zmq_output_mode", "native_ft"),
                               ("zmq_framing", "single"), ("zmq_queue_capacity", "256")]:
                text = field(text, key, value)
        config.write_text(text)
        copied.append(config)
    columns = math.ceil(math.sqrt(len(copied)))
    rows = math.ceil(len(copied) / columns)
    command = [
        str(args.binary.resolve()), "--config", str(copied[0]),
        "--windows", str(len(copied)), "--grid", f"{columns}x{rows}",
        "--control-stdio", "--window-size", "520x480",
    ]
    if not args.no_screenshot:
        command += ["--screenshot", str(output / "native-root.png"), "--screenshot-after", "3"]
    probe = None
    if args.zmq_base_port is not None:
        from native_zmq_probe import NativeWireProbe
        probe = NativeWireProbe(range(args.zmq_base_port, args.zmq_base_port + len(copied)))
    bridge = NativeBridge(command, output)
    wire_directories = []
    report = {"command": command, "configs": [str(p.resolve()) for p in args.configs],
              "working_directory": str(output),
              "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "copied_config_sha256": {str(path): hashlib.sha256(path.read_bytes()).hexdigest()
                                       for path in copied},
              "media_recording": "disabled; optional wire oracle records numeric data only",
              "capture_drop_scope": "application delivered sequence; zero does not establish zero sensor/kernel drops",
              "display": {k: os.environ.get(k) for k in ["DISPLAY", "WAYLAND_DISPLAY"]}}
    try:
        schema = bridge.request("discover")
        report["transport"] = schema["transport"]
        sessions = bridge.request("sessions")
        assert len(sessions) == len(copied), sessions
        for session in sessions:
            bridge.request("start", session["session"])
        sessions = bridge.wait(lambda states: all(s["frames"] >= 5 for s in states))
        report["startup"] = sessions
        assert all(s["camera"] is not None and s["native_window"] for s in sessions), sessions
        ids = [s["session"] for s in sessions]
        if args.display_fps is not None:
            for identity, fps in zip(ids, args.display_fps):
                bridge.request("view", identity, display_fps=fps)
            applied = bridge.wait(lambda states: all(
                s["preview_target_fps"] == fps for s, fps in zip(states, args.display_fps)))
            assert all(s["revision"] == b["revision"] and s["source_fps"] == b["source_fps"]
                       and s["source"] == b["source"] for s, b in zip(applied, sessions)), applied
            report["display_limits"] = {"requested": args.display_fps, "sessions": applied}

        if not args.no_screenshot:
            # Preserve a readable native framebuffer before the tiny-window cases.
            screenshot_deadline = time.monotonic() + 10
            while not (output / "native-root.png").exists():
                if time.monotonic() >= screenshot_deadline:
                    raise RuntimeError("native framebuffer screenshot was not delivered")
                bridge.request("sessions")
                time.sleep(0.1)

        # Numeric source selection calls the same Session::select_camera used
        # by the camera menu. Selecting the active camera must keep its epoch.
        before_selection = bridge.request("sessions")
        for session in before_selection:
            bridge.request("source", session["session"], input=session["source"])
        same_source = bridge.wait(lambda states: all(
            s["frames"] > b["frames"] for s, b in zip(states, before_selection)
        ))
        assert all(s["revision"] == b["revision"] and s["source"] == b["source"]
                   for s, b in zip(same_source, before_selection)), same_source
        report["select_active_camera"] = same_source
        if len(ids) > 1:
            rejected = bridge.request("source", ids[1], expect_error=True,
                                      input=before_selection[0]["source"])
            assert "already used" in rejected["message"], rejected
            unchanged = bridge.request("sessions")
            assert unchanged[1]["source"] == before_selection[1]["source"], unchanged
            assert unchanged[1]["frames"] >= same_source[1]["frames"], unchanged
            report["occupied_camera_rejected"] = {"error": rejected, "sessions": unchanged}

        # Release and reacquire each camera through actual source selection,
        # proving that repeated menu reconnects do not leave blank sessions.
        sample = Path(__file__).resolve().parents[1] / "examples" / "sample.mp4"
        report["camera_reconnections"] = []
        for cycle in range(args.reconnect_cycles):
            for initial in before_selection:
                identity, camera = initial["session"], initial["source"]
                other_before = bridge.request("sessions")
                bridge.request("source", identity, input=str(sample))
                bridge.wait(lambda states: any(
                    s["session"] == identity and s["camera"] is None and s["frames"] >= 2
                    for s in states
                ))
                bridge.request("source", identity, input=camera)
                restored = bridge.wait(lambda states: any(
                    s["session"] == identity and s["source"] == camera
                    and s["camera"] is not None and s["frames"] >= 5 for s in states
                ))
                assert all(s["frames"] > b["frames"] for s, b in zip(restored, other_before)
                           if s["session"] != identity), restored
                report["camera_reconnections"].append({"cycle": cycle + 1, "session": identity,
                                                       "sessions": restored})

        # Pausing one capture must not stop progress in any other worker.
        bridge.request("pause", ids[0])
        paused = bridge.wait(lambda states: states[0]["paused"])
        before = {s["session"]: s["frames"] for s in paused}
        time.sleep(1)
        independent = bridge.request("sessions")
        assert independent[0]["frames"] == before[ids[0]], independent
        assert all(s["frames"] > before[s["session"]] for s in independent[1:]), independent
        bridge.request("start", ids[0])
        bridge.wait(lambda states: all(s["frames"] > before[s["session"]] for s in states))
        report["pause_isolation"] = independent

        report["resizes"] = []
        for resize_cycle in range(args.resize_cycles):
            for diagnostics in [False, True]:
                for width, height in [(96, 96), (180, 140), (640, 480), (1000, 250), (520, 480)]:
                    for identity in ids:
                        bridge.request("view", identity, window_size=[width, height], diagnostics=diagnostics)
                    resized = bridge.wait(lambda states: all(
                        s["window_inner_size"] is not None
                        # Native child windows can reserve server/client decoration
                        # margins. The tested Wayland compositor constrains a 96x96
                        # request to 104x128; larger windows differ by 8x32 as well.
                        and 96 <= s["window_inner_size"][0]
                        and 96 <= s["window_inner_size"][1]
                        and abs(s["window_inner_size"][0] - width) <= 40
                        and abs(s["window_inner_size"][1] - height) <= 40
                        for s in states
                    ))
                    report["resizes"].append({"cycle": resize_cycle + 1, "requested": [width, height], "diagnostics": diagnostics,
                                             "sessions": resized})
                    if resize_cycle == 0 and (width, height) == (96, 96) and not diagnostics and args.tiny_seconds > 0:
                        tiny_start = time.monotonic()
                        time.sleep(args.tiny_seconds)
                        tiny_final = bridge.request("sessions")
                        tiny_elapsed = time.monotonic() - tiny_start
                        assert all(s["frames"] > b["frames"] and not s["ended"] and not s["error"]
                                   for s, b in zip(tiny_final, resized)), tiny_final
                        report["tiny_window_hold"] = {
                            "elapsed_seconds": tiny_elapsed,
                            "progress": frame_progress(resized, tiny_final, tiny_elapsed),
                            "sessions": tiny_final,
                        }
        for identity in ids:
            bridge.request("view", identity, diagnostics=False)
        if probe is not None:
            for identity in ids:
                directory = output / "wire-oracle" / f"VR{identity}"
                wire_directories.append(directory)
                bridge.request("record_start", identity, directory=str(directory),
                               record_options={"raw": False, "debug": False})
            bridge.wait(lambda states: all(s["recording"] for s in states))
        baseline = bridge.request("sessions")
        observation_start = time.monotonic()
        time.sleep(args.seconds)
        final = bridge.request("sessions")
        observation_elapsed = time.monotonic() - observation_start
        report["final"] = final
        if args.display_fps is not None:
            assert all(s["preview_target_fps"] == fps and s["source_fps"] == b["source_fps"]
                       for s, fps, b in zip(final, args.display_fps, report["startup"])), final
        assert all(s["frames"] > b["frames"] for s, b in zip(final, baseline)), final
        assert all(s["last_row"] is not None and not s["ended"] and not s["error"] for s in final), final
        report["requested_observation_seconds"] = args.seconds
        report["elapsed_observation_seconds"] = observation_elapsed
        report["observation_progress"] = frame_progress(baseline, final, observation_elapsed)
        if probe is not None:
            for identity in ids:
                bridge.request("record_stop", identity)
            bridge.wait(lambda states: all(not s["recording"] for s in states))
            assert all(s["outputs"]["zmq"]["worker_alive"] for s in final), final
        report["passed"] = True
        # Close a child first, then the event-loop root; recording workers join cleanly.
        for identity in reversed(ids):
            bridge.request("close", identity, discard=True)
        bridge.process.wait(timeout=15)
        assert bridge.process.returncode == 0, bridge.process.returncode
        if probe is not None:
            report["zmq_load"] = probe.finish()
            report["zmq_load"]["recorded_wire_oracle"] = probe.compare_recordings(wire_directories)
            probe = None
    except Exception as error:
        report["passed"] = False
        report["failure"] = str(error)
        raise
    finally:
        if probe is not None:
            try:
                report["zmq_load"] = probe.finish()
            except Exception as error:
                report["zmq_load_error"] = str(error)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        bridge.finish(range(1, len(copied) + 1))
    print(json.dumps({"passed": True, "report": str(output / "report.json"),
                      "frames": [s["frames"] for s in report["final"]]}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
    parser.add_argument("--configs", type=Path, nargs="+", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seconds", type=float, default=5)
    parser.add_argument("--zmq-base-port", type=int,
                        help="Enable isolated native FT publishers plus healthy, undrained, and incomplete peers; requires PyZMQ")
    parser.add_argument("--no-screenshot", action="store_true",
                        help="Skip GPU readback only when diagnosing a screenshot-specific backend failure")
    parser.add_argument("--display-fps", type=int, nargs="+",
                        help="One independent native display limit (1..60) per config; capture/processing rates remain unchanged")
    parser.add_argument("--tiny-seconds", type=float, default=15,
                        help="Hold all windows at their minimum size before continuing resize checks")
    parser.add_argument("--resize-cycles", type=int, default=3,
                        help="Repeat the ten-size/diagnostics transition matrix this many times")
    parser.add_argument("--reconnect-cycles", type=int, default=1,
                        help="Switch every camera to the bundled video and back this many times")
    arguments = parser.parse_args()
    if arguments.zmq_base_port is not None and not 1024 <= arguments.zmq_base_port <= 65536 - len(arguments.configs):
        parser.error("--zmq-base-port must leave one distinct unprivileged port per rig")
    if not 1 <= len(arguments.configs) <= 16:
        parser.error("provide 1..16 rig configs")
    if arguments.display_fps is not None and (len(arguments.display_fps) != len(arguments.configs)
            or any(not 1 <= fps <= 60 for fps in arguments.display_fps)):
        parser.error("--display-fps requires one integer within 1..60 per config")
    if not math.isfinite(arguments.seconds) or not 0 < arguments.seconds <= 3600:
        parser.error("--seconds must be finite and within 0..3600")
    if not math.isfinite(arguments.tiny_seconds) or not 0 <= arguments.tiny_seconds <= 3600:
        parser.error("--tiny-seconds must be finite and within 0..3600 (zero skips the hold)")
    if not 1 <= arguments.resize_cycles <= 20:
        parser.error("--resize-cycles must be within 1..20")
    if not 0 <= arguments.reconnect_cycles <= 10:
        parser.error("--reconnect-cycles must be within 0..10")
    run(arguments)
