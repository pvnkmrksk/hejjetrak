#!/usr/bin/env python3
"""Development-only sustained native-camera recording and read-only health checks.

The Rust application remains the capture, tracking, encoding and output runtime.
This supervisor controls its documented JSONL bridge and independently audits
completed takes. Original configs/input files are never modified or deleted.
"""
from __future__ import annotations

import argparse
from collections import Counter, deque
from concurrent.futures import ThreadPoolExecutor, TimeoutError as FutureTimeoutError
import csv
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import signal
import shutil
import subprocess
import sys
import threading
import time
from zoneinfo import ZoneInfo

from benchmark_vr3 import dat_rows
from recording_matrix import process_metrics, verify_recording
from test_native_rigs import NativeBridge, field

GIB = 1024 ** 3
BERLIN = ZoneInfo("Europe/Berlin")


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def atomic_json(path, value):
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")
    temporary.replace(path)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def pid_identity(pid):
    try:
        fields = (Path("/proc") / str(pid) / "stat").read_text().rsplit(")", 1)[1].split()
        if fields[0] == "Z":
            return None
        return {"pid": pid, "start_ticks": int(fields[19])}
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return None


def compact_state(state):
    keys = ("session", "source", "source_label", "frames", "revision", "requested_revision",
            "paused", "ended", "recording", "recording_directory", "record_options",
            "actual_fps", "source_fps", "tracking_ms", "tracking_samples", "processing_ms",
            "preview_fps", "preview_target_fps", "headroom_percent", "last_row", "error",
            "command_error", "notice", "media_warnings", "media_recording", "outputs",
            "recording_finalizations")
    return {key: state.get(key) for key in keys}


class CompactBridge(NativeBridge):
    """Same native operations, with bounded status transcripts instead of camera-format dumps."""

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
            raise RuntimeError(f"native {command} request timed out after30s") from error
        if response is None:
            raise RuntimeError(f"native process exited {self.process.poll()}; inspect stderr.log")
        if response.get("id") != self.counter:
            raise RuntimeError(f"native response ID mismatch for {command}")
        result = response.get("result")
        logged = {"id": response.get("id"), "ok": response.get("ok"), "error": response.get("error")}
        if command == "sessions" and isinstance(result, list):
            logged["result"] = [compact_state(state) for state in result]
        elif isinstance(result, dict):
            logged["result"] = {key: value for key, value in result.items() if key != "status"}
            if isinstance(result.get("status"), dict):
                logged["result"]["status"] = compact_state(result["status"])
        else:
            logged["result"] = result
        self.transcript.write(json.dumps({"at": utc_now(), "request": request, "response": logged}) + "\n")
        self.transcript.flush()
        if expect_error:
            if response.get("ok"):
                raise RuntimeError("native request unexpectedly succeeded")
            return response["error"]
        if not response.get("ok"):
            raise RuntimeError(f"native {command} failed: {response.get('error')}")
        return result


def percentile(values, fraction):
    if not values:
        return None
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, max(0, math.ceil(len(ordered) * fraction) - 1))]


def trajectory_statistics(directory):
    rows = dat_rows(next(directory.glob("*-all-frames.dat")))
    with next(directory.glob("*-quality.csv")).open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    if len(rows) != len(quality) or not rows:
        raise RuntimeError("empty or mismatched numerical/quality take")
    states = Counter(item["status"] for item in quality)
    motion = [(row, decision) for row, decision in zip(rows, quality)
              if decision["status"] == "tracked" and decision["accepted"] == "true" and row[23] > 0]
    angular = [math.sqrt(sum(value * value for value in row[1:4])) / (row[23] / 1000)
               for row, _ in motion]
    speed = [row[18] / (row[23] / 1000) for row, _ in motion]
    errors = [float(item["fit_error"]) for item in quality]
    intervals = [right[21] - left[21] for left, right in zip(rows, rows[1:])]
    if any(b[0] != a[0] + 1 or b[21] <= a[21] for a, b in zip(rows, rows[1:])):
        raise RuntimeError("nonconsecutive measured frame counters or nonmonotonic clocks")
    run = longest = 0
    for decision in quality:
        run = run + 1 if decision["accepted"] != "true" else 0
        longest = max(longest, run)
    return {"rows": len(rows), "first_frame": int(rows[0][0]), "last_frame": int(rows[-1][0]),
            "first_timestamp_ms": rows[0][21], "last_timestamp_ms": rows[-1][21],
            "accepted_fraction": sum(item["accepted"] == "true" for item in quality) / len(rows),
            "quality_statuses": dict(states), "longest_unaccepted_run": longest,
            "motion_measurements": len(motion), "missing_motion_rows": len(rows) - len(motion),
            "fit_error_median": percentile(errors, .5), "fit_error_p95": percentile(errors, .95),
            "angular_speed_rad_per_second_p50": percentile(angular, .5),
            "angular_speed_rad_per_second_p95": percentile(angular, .95),
            "angular_speed_rad_per_second_p99": percentile(angular, .99),
            "angular_speed_rad_per_second_maximum": max(angular, default=None),
            "translation_sphere_rad_per_second_p50": percentile(speed, .5),
            "translation_sphere_rad_per_second_p95": percentile(speed, .95),
            "translation_sphere_rad_per_second_maximum": max(speed, default=None),
            "source_interval_ms_p50": percentile(intervals, .5),
            "source_interval_ms_p95": percentile(intervals, .95),
            "source_interval_ms_maximum": max(intervals, default=None),
            "source_intervals_above_50ms": sum(interval > 50 for interval in intervals),
            "nominal_missing_interval_estimate": sum(max(0, round(interval / 10) - 1) for interval in intervals),
            "first_position": rows[0][14:16], "last_position": rows[-1][14:16],
            "boundary": "Velocity uses tracked relative measurements and actual recorded delta-time only. Rejections/relocalization are missing motion, never measured zero. Live motion has no independent physical truth."}


def clock_warnings(stats):
    warnings = []
    if stats["source_intervals_above_50ms"]:
        warnings.append(f"source clock has {stats['source_intervals_above_50ms']} interval(s) above 50 ms within the take; largest {stats['source_interval_ms_maximum']:.3f} ms. Inspect acquisition/file I/O; this is not a verified sensor-drop count")
    rotation = stats.get("rotation_source_interval_ms")
    if rotation is not None and rotation > 50:
        warnings.append(f"source clock rotation interval is {rotation:.3f} ms, above 50 ms. Inspect acquisition/file I/O; this is not a verified sensor-drop count")
    return warnings


def completion_marker(directory):
    path = directory / "completion.json"
    if not path.exists():
        return None
    value = json.loads(path.read_text())
    if Path(value["directory"]).resolve() != directory.resolve():
        raise RuntimeError("recording completion marker identifies a different take")
    if value["state"] == "failed" or value.get("numeric_error") or value.get("completion_error"):
        raise RuntimeError(f"recording finalization failed: {value}")
    return value if value["state"] == "complete" else None


def await_completion(output, directory, required, timeout=120):
    if not required:
        return {"mode": "legacy synchronous recording finalization"}
    limit = time.monotonic() + timeout
    while True:
        marker = completion_marker(directory)
        if marker:
            return marker
        status_path = output / "status.json"
        if status_path.exists():
            status = json.loads(status_path.read_text())
            for session in status.get("sessions", []):
                for item in session.get("recording_finalizations", []) or []:
                    if Path(item["directory"]).resolve() == directory.resolve() and item.get("completion_error"):
                        raise RuntimeError(f"recording completion marker could not be saved: {item['completion_error']}")
        if time.monotonic() >= limit:
            raise RuntimeError(f"recording completion marker is still missing/pending after {timeout}s: {directory}")
        time.sleep(.1)


def numeric_bytes(directories):
    return sum(path.stat().st_size for directory in directories.values()
               for path in directory.glob("epoch-*") if path.is_file())


def require_generated_directory(output, directory):
    """Fail closed before any movie-pruning helper can touch an external path."""
    base = (output / "segments").resolve()
    resolved = directory.resolve()
    if not resolved.is_relative_to(base) or resolved == base or directory.is_symlink():
        raise ValueError("refusing to verify/prune a directory outside this run's generated segments")
    for path in directory.rglob("*"):
        if path.is_symlink() or not path.resolve().is_relative_to(base):
            raise ValueError("refusing to verify/prune a generated tree containing symlinks")


def storage_projection(free, numeric_rate, remaining_seconds, camera_count, probe_seconds):
    # Numerical rows have two full25-column files plus quality/config evidence.
    protected_numerics = math.ceil(max(numeric_rate, camera_count * 100 * 1000) * remaining_seconds * 1.15)
    probe = math.ceil(camera_count * 100 * 3 * (320 * 240 + 640 * 480) * probe_seconds * 1.1)
    return {"free_bytes": free, "remaining_numeric_budget_bytes": protected_numerics,
            "fixed_reserve_bytes": 2 * GIB, "probe_budget_bytes": probe,
            "raw_probe_allowed": free > protected_numerics + 2 * GIB + probe,
            "source_video_allowed": free > protected_numerics + 3 * GIB,
            "numerics_at_risk": free < GIB,
            "boundary": "Measured numerical byte rate with1KiB/frame floor and15% margin;2GiB reserve. Video retention is bounded separately. Other writers/quotas remain outside this projection."}


def exact_wire_case(probe, port, directory):
    expected = next(directory.glob("epoch-*.dat"))
    expected = directory / "epoch-0000.dat" if (directory / "epoch-0000.dat").exists() else expected
    rows = expected.read_bytes().splitlines(keepends=True)
    index = {b"FT, " + row: number for number, row in enumerate(rows)}
    # Builtin byte-deque copying is bounded and owns the sampled packet bytes.
    for attempt in range(3):
        try:
            packets = list(probe.packets[port])
            break
        except RuntimeError:
            if attempt == 2:
                raise
    matched = [index[packet] for packet in packets if packet in index]
    return {"port": port, "expected_accepted_rows": len(rows), "matched_rows": len(matched),
            "exact_in_order_without_duplicates": matched == list(range(len(rows))),
            "expected_ft_sha256": hashlib.sha256(b"".join(b"FT, " + row for row in rows)).hexdigest(),
            "received_packet_counter": probe.received[port],
            "boundary": "Independent modern libzmq subscriber, exact accepted-row bytes; bounded100000-packet retention per rig. Missing matches are reported, never inferred from publisher dispatch counts."}


def verify_group(output, group, previous, probe, ports):
    started = time.monotonic()
    result = {"index": group["index"], "phase": group["phase"], "hour": group["hour"],
              "keep_video": group["keep_video"], "at": utc_now(), "sessions": [], "warnings": []}
    final_completions = {}
    if group.get("final_source_candidate"):
        for identity, directory in group["directories"].items():
            try:
                require_generated_directory(output, directory)
                final_completions[identity] = await_completion(output, directory, group.get("async_finalization", False))
            except Exception as error:
                final_completions[identity] = error
        if all(not isinstance(value, Exception) for value in final_completions.values()):
            movie_bytes = sum(path.stat().st_size for directory in group["directories"].values()
                              for path in directory.glob("*-media/*.avi") if path.is_file())
            result["keep_video"] = movie_bytes < GIB and shutil.disk_usage(output).free > 2 * GIB
    for identity, directory in group["directories"].items():
        item = {"session": identity, "directory": str(directory)}
        try:
            require_generated_directory(output, directory)
            marker = final_completions.get(identity)
            if isinstance(marker, Exception):
                raise marker
            item["completion"] = marker or await_completion(output, directory, group.get("async_finalization", False))
            stats = trajectory_statistics(directory)
            prior = previous.get(identity)
            if prior:
                if stats["first_frame"] != prior["last_frame"] + 1 or stats["first_timestamp_ms"] <= prior["last_timestamp_ms"]:
                    raise RuntimeError("recording rotation lost/duplicated numerical frame IDs or source clock continuity")
                stats["rotation_source_interval_ms"] = stats["first_timestamp_ms"] - prior["last_timestamp_ms"]
            previous[identity] = stats
            item["trajectory"] = stats
            result["warnings"].extend(f"session{identity}: {warning}" for warning in clock_warnings(stats))
            if stats["rows"] >= 100 and stats["accepted_fraction"] < .95:
                result["warnings"].append(f"session{identity}: only {stats['accepted_fraction']:.1%} of this segment accepted; inspect tracking/occlusion, no reset attempted")
            if stats["longest_unaccepted_run"] >= 100:
                result["warnings"].append(f"session{identity}: at least100 consecutive frames have unmeasured motion")
            if (stats["angular_speed_rad_per_second_p99"] or 0) > 150:
                result["warnings"].append(f"session{identity}: angular velocity p99 exceeds150rad/s; inspect possible runaway fit (not a physical-ground-truth test)")
            if probe:
                item["wire"] = exact_wire_case(probe, ports[identity], directory)
                if not item["wire"]["exact_in_order_without_duplicates"]:
                    result["warnings"].append(f"session{identity}: accepted-wire match incomplete; preserve numeric/source evidence")
            # Complete source/debug files must match every numerical row. Partial
            # prefixes are retained as warnings and are never pruned as successes.
            audited = verify_recording(directory, group["options"], allow_partial=True,
                                       discard_verified_video=False,
                                       expected_codec=group["codec"], expected_encoding=group["encoding"])
            item["recording"] = audited
            item["verified"] = True
            if not audited["all_requested_video_complete"]:
                result["warnings"].append(f"session{identity}: optional video stopped with explicit partial prefix")
            elif not result["keep_video"] and any(group["options"].values()):
                # The shared verifier rechecks counts/identity, records SHA256,
                # extracts full-frame first/last images, then unlinks only video.
                item["recording"] = verify_recording(directory, group["options"], allow_partial=False,
                                                     discard_verified_video=True,
                                                     expected_codec=group["codec"], expected_encoding=group["encoding"])
        except Exception as error:
            item["verified"] = False
            item["error"] = str(error)
            result["warnings"].append(f"session{identity}: verification failed: {error}; all artifacts retained")
        result["sessions"].append(item)
    result["elapsed_seconds"] = time.monotonic() - started
    result["verified"] = all(item["verified"] for item in result["sessions"])
    result["numeric_bytes"] = numeric_bytes(group["directories"])
    atomic_json(output / "verifications" / f"segment-{group['index']:06}.json", result)
    return result


def start_inhibitor(output, seconds):
    executable = shutil.which("systemd-inhibit")
    if not executable:
        return None, {"acquired": False, "error": "systemd-inhibit is unavailable; keep the machine awake manually"}
    who = f"hejjetrac-soak-{os.getpid()}"
    log = (output / "sleep-inhibitor.stderr").open("w")
    command = [executable, "--what=sleep", "--mode=block", "--who=" + who,
               "--why=Continuous camera tracking and recording validation", "sleep", str(math.ceil(seconds + 120))]
    process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
    time.sleep(.3)
    try:
        listed = subprocess.run([executable, "--list", "--no-pager", "--no-legend"],
                                capture_output=True, text=True, timeout=10)
        own_lines = [line for line in listed.stdout.splitlines() if who in line]
    except (OSError, subprocess.TimeoutExpired):
        own_lines = []
    acquired = process.poll() is None and bool(own_lines)
    return (process, log), {"acquired": acquired, "process": pid_identity(process.pid),
                           "command": command, "own_inhibitor_entries": own_lines,
                           "error": None if acquired else "sleep inhibition not confirmed; inspect sleep-inhibitor.stderr"}


def close_inhibitor(inhibitor):
    if inhibitor:
        process, log = inhibitor
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            process.wait(timeout=5)
        log.close()


def snapshot_hour(bridge, identities, output, hour):
    saved = []
    for identity in identities:
        destination = output / "snapshots" / hour / f"session-{identity}"
        destination.parent.mkdir(parents=True, exist_ok=True)
        bridge.request("snapshot", identity, directory=str(destination))
        saved.append(str(destination))
    return {"hour": hour, "directories": saved,
            "boundary": "Running preview snapshots are approximate visual annotations; source images may predate current numeric metadata by the chosen display cadence. Full numerical trajectories are retained separately."}


def parse_deadline(args):
    if args.duration_seconds is not None:
        if args.duration_seconds <= 0:
            raise ValueError("duration must be positive")
        return datetime.now(timezone.utc) + timedelta(seconds=args.duration_seconds)
    deadline = datetime.fromisoformat(args.until)
    if deadline.tzinfo is None:
        deadline = deadline.replace(tzinfo=BERLIN)
    if deadline <= datetime.now(timezone.utc):
        raise ValueError("deadline must be in the future")
    return deadline


def run_soak(args):
    output = args.output.resolve()
    if args.prepared:
        if not (output / "launch.json").is_file() or (output / "runner.pid").exists():
            raise ValueError("prepared launch requires its fresh launch marker and no previous runner")
    else:
        output.mkdir(parents=True, exist_ok=False, mode=0o700)
    for name in ("configs", "segments", "verifications"):
        (output / name).mkdir(exist_ok=False)
    deadline = parse_deadline(args)
    identities, bridge, inhibitor, probe = [], None, None, None
    stop = threading.Event()
    signal.signal(signal.SIGTERM, lambda *_: stop.set())
    signal.signal(signal.SIGINT, lambda *_: stop.set())
    (output / "runner.pid").write_text(str(os.getpid()) + "\n")
    state = {"state": "starting", "started_utc": utc_now(), "updated_utc": utc_now(),
             "deadline": deadline.isoformat(), "runner": pid_identity(os.getpid()), "application": None,
             "binary": str(args.binary.resolve()), "binary_sha256": sha256(args.binary),
             "sensor_zero_loss_claimed": False, "warnings": [], "completed_segments": 0,
             "recording_rotation_resets_tracker": False, "retention": {
                 "all_numeric_quality_config_retained": True, "full_source_frame_saved": True,
                 "one_complete_source_clip_per_wall_hour_retained": True,
                 "debug_probe_movies": "verify full source/debug counts then retain SHA256/first+last full-frame PNGs, prune only successful generated movies",
                 "unverified_or_partial_video_deleted": False}}
    atomic_json(output / "status.json", state)
    pending, completed, previous = [], [], {}
    closed_numeric = 0
    sample_started = time.monotonic()
    stop_reason = "deadline"
    executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="media-verifier")
    telemetry = (output / "telemetry.jsonl").open("w")
    try:
        if args.inhibit:
            inhibitor, state["sleep_inhibition"] = start_inhibitor(output, (deadline - datetime.now(timezone.utc)).total_seconds())
            if not state["sleep_inhibition"]["acquired"]:
                state["warnings"].append(state["sleep_inhibition"]["error"])
        if args.wire:
            from native_zmq_probe import NativeWireProbe
            ports = list(range(args.wire_port_base, args.wire_port_base + len(args.configs)))
            probe = NativeWireProbe(ports)
            # Bounded16-minute window permits delayed low-priority media audits.
            probe.packets = {port: deque(maxlen=100000) for port in ports}
        copied = []
        for index, original in enumerate(args.configs, 1):
            text = original.read_text()
            replacements = [("save_raw", "n"), ("save_debug", "n"), ("sock_port", "0"),
                            ("com_port", ""), ("zmq_port", str(args.wire_port_base + index - 1) if args.wire else "0"),
                            ("zmq_host", "127.0.0.1"), ("zmq_output_mode", "native_ft"),
                            ("zmq_framing", "single"), ("zmq_queue_capacity", "256")]
            if args.fixture_video:
                replacements.extend([("src_fn", str(args.fixture_video.resolve())), ("src_fps", str(args.fixture_fps))])
            for key, value in replacements:
                text = field(text, key, value)
            config = output / "configs" / f"VR{index}" / "config.txt"
            config.parent.mkdir(); config.write_text(text); copied.append(config)
        state["configs"] = [{"original": str(path.resolve()), "original_sha256": sha256(path),
                             "effective": str(copy), "effective_sha256": sha256(copy)}
                            for path, copy in zip(args.configs, copied)]
        count = len(copied)
        columns = math.ceil(math.sqrt(count)); rows = math.ceil(count / columns)
        command = [str(args.binary.resolve()), "--config", str(copied[0]), "--windows", str(count),
                   "--grid", f"{columns}x{rows}", "--window-size", "480x360", "--control-stdio"]
        bridge = CompactBridge(command, output)
        state["application"] = pid_identity(bridge.process.pid)
        # Lower only this analysis supervisor and future verifier subprocesses;
        # the already launched native application's scheduling remains unchanged.
        try:
            os.nice(5)
        except OSError:
            pass
        sessions = bridge.request("sessions")
        identities = [session["session"] for session in sessions]
        if len(identities) != count:
            raise RuntimeError("native session count differs from requested rigs")
        for identity in identities:
            bridge.request("view", identity, display_fps=args.display_fps)
            bridge.request("start", identity)
        sessions = bridge.wait(lambda states: all(session["frames"] >= 10 for session in states))
        if not args.fixture_video and any(session["camera"] is None or session["source_fps"] != 100 for session in sessions):
            raise RuntimeError("night run requires all calibrated live100FPS cameras")
        revisions = {session["session"]: session["revision"] for session in sessions}
        sources = {session["session"]: session["source"] for session in sessions}
        async_finalization = all("recording_finalizations" in session for session in sessions)
        state["asynchronous_recording_finalization"] = async_finalization
        port_map = {identity: args.wire_port_base + index for index, identity in enumerate(identities)}
        current, index, debug_due, snapshots = None, 0, None, set()
        hourly_reserved = set()
        previous_sample, previous_states = process_metrics(bridge.process.pid), sessions
        previous_tick = time.monotonic()
        next_poll = previous_tick
        latest_verifications = []
        while datetime.now(timezone.utc) < deadline and not stop.is_set():
            if (output / "stop.request").exists():
                stop_reason = "requested graceful stop"; break
            now = time.monotonic()
            for future, group in list(pending):
                if future.done():
                    verified = future.result(); pending.remove((future, group)); completed.append(verified)
                    closed_numeric += verified["numeric_bytes"] - group.get("numeric_bytes_at_rotation", 0)
                    latest_verifications = completed[-4:]
                    for warning in verified["warnings"]:
                        state["warnings"].append(warning)
            elapsed = now - sample_started
            directories = current["directories"] if current else {}
            current_numeric = numeric_bytes(directories)
            numeric_rate = (closed_numeric + current_numeric) / max(1, elapsed)
            remaining = max(0, (deadline - datetime.now(timezone.utc)).total_seconds())
            projection = storage_projection(shutil.disk_usage(output).free, numeric_rate, remaining,
                                            count, args.debug_probe_seconds)
            if projection["numerics_at_risk"]:
                stop_reason = "less than1GiB remains; stopping gracefully to protect numerical evidence"
                state["warnings"].append(stop_reason)
                break
            if current is None or now >= current["end_monotonic"]:
                hour = datetime.now(BERLIN).strftime("%Y%m%dT%H")
                phase = "debug_probe" if debug_due == hour and projection["raw_probe_allowed"] else "source"
                if debug_due == hour and not projection["raw_probe_allowed"]:
                    state["warnings"].append("hourly debug probe skipped to protect projected numerical capacity")
                    debug_due = None
                video_allowed = projection["source_video_allowed"] and len(pending) < 2
                if phase == "debug_probe":
                    options, encoding, codec, duration, keep = {"raw": True, "debug": True}, "raw", "raw", args.debug_probe_seconds, False
                    debug_due = None
                else:
                    options, encoding, codec, duration = {"raw": video_allowed, "debug": False}, "mjpeg", "mjpg", args.segment_seconds
                    keep = video_allowed and hour not in hourly_reserved
                    if keep:
                        hourly_reserved.add(hour); debug_due = hour
                    if not video_allowed:
                        state["warnings"].append("new segment is data-only: video/verification capacity or numerical storage projection is constrained")
                new_directories = {identity: output / "segments" / f"{index:06}" / f"session-{identity}" for identity in identities}
                for identity in identities:
                    bridge.request("record_start", identity, directory=str(new_directories[identity]),
                                   record_options=options, recording_tuning={"encoding": encoding})
                before_rotation = time.monotonic()
                sessions = bridge.wait(lambda states: all(session["recording"] and session["recording_directory"] == str(new_directories[session["session"]]) for session in states))
                if current:
                    current["numeric_bytes_at_rotation"] = numeric_bytes(current["directories"])
                    closed_numeric += current["numeric_bytes_at_rotation"]
                    current["finalized_utc"] = utc_now()
                    pending.append((executor.submit(verify_group, output, current, previous, probe, port_map), current))
                current = {"index": index, "phase": phase, "hour": hour, "directories": new_directories,
                           "async_finalization": async_finalization,
                           "options": options, "encoding": encoding, "codec": codec, "keep_video": keep,
                           "started_utc": utc_now(), "end_monotonic": time.monotonic() + duration,
                           "rotation_ack_seconds": time.monotonic() - before_rotation}
                index += 1
                if hour not in snapshots:
                    try:
                        state.setdefault("snapshots", []).append(snapshot_hour(bridge, identities, output, hour))
                    except Exception as error:
                        state["warnings"].append(f"hourly visual snapshot failed: {error}")
                    snapshots.add(hour)
            if now >= next_poll:
                sessions = bridge.request("sessions")
                metrics = process_metrics(bridge.process.pid); tick = time.monotonic(); span = tick - previous_tick
                deltas = {session["session"]: session["frames"] - old["frames"] for session, old in zip(sessions, previous_states)}
                for session in sessions:
                    identity = session["session"]
                    if session["error"] or session["command_error"] or session["ended"] or session["paused"]:
                        raise RuntimeError(f"session{identity} stopped/paused/errored: {compact_state(session)}")
                    if session["revision"] != revisions[identity] or session["source"] != sources[identity]:
                        raise RuntimeError(f"session{identity} source/config changed during uninterrupted soak")
                    if deltas[identity] <= 0 and span >= args.poll_seconds * .8:
                        raise RuntimeError(f"session{identity} made no numerical progress")
                    if not session["recording"]:
                        raise RuntimeError(f"session{identity} recording stopped unexpectedly")
                if probe and probe.errors:
                    state["warnings"].append("live subscriber reported an error: " + "; ".join(probe.errors[-4:]))
                current_media_warnings = [f"session{session['session']}: {warning}" for session in sessions for warning in session.get("media_warnings", [])]
                state["warnings"].extend(current_media_warnings)
                state.update({"state": "running", "updated_utc": utc_now(), "elapsed_seconds": elapsed,
                              "sessions": [compact_state(session) for session in sessions],
                              "interval_seconds": span, "processed_fps": {str(k): value / span for k, value in deltas.items()},
                              "application_cpu_percent_one_core": (metrics["cpu_seconds"] - previous_sample["cpu_seconds"]) * 100 / span,
                              "process": metrics, "storage": projection, "numeric_bytes": closed_numeric + current_numeric,
                              "current_segment": {key: str(value) if isinstance(value, Path) else value for key, value in current.items() if key != "directories"},
                              "current_directories": {str(k): str(v) for k, v in current["directories"].items()},
                              "completed_segments": len(completed), "verification_backlog": len(pending),
                              "latest_verifications": latest_verifications,
                              "wire": {"received": probe.received.copy(), "errors": probe.errors[-4:]} if probe else None,
                              "warnings": list(dict.fromkeys(state["warnings"]))[-30:]})
                atomic_json(output / "status.json", state)
                telemetry.write(json.dumps({key: value for key, value in state.items() if key not in ("configs", "snapshots", "latest_verifications", "retention")}) + "\n"); telemetry.flush()
                previous_sample, previous_states, previous_tick = metrics, sessions, tick
                next_poll = tick + args.poll_seconds
            stop.wait(min(.2, max(0, next_poll - time.monotonic())))
        if bridge:
            state["state"] = "finalizing"; state["updated_utc"] = utc_now()
            atomic_json(output / "status.json", state)
            for identity in identities:
                bridge.request("record_stop", identity)
            bridge.wait(lambda states: all(not session["recording"] for session in states))
            if current:
                current["numeric_bytes_at_rotation"] = numeric_bytes(current["directories"])
                closed_numeric += current["numeric_bytes_at_rotation"]
                current["final_source_candidate"] = current["phase"] == "source" and current["options"]["raw"]
                pending.append((executor.submit(verify_group, output, current, previous, probe, port_map), current))
            for future, group in pending:
                limit = time.monotonic() + 240
                while True:
                    try:
                        verified = future.result(timeout=15)
                        completed.append(verified)
                        closed_numeric += verified["numeric_bytes"] - group.get("numeric_bytes_at_rotation", 0)
                        break
                    except FutureTimeoutError:
                        state["updated_utc"] = utc_now()
                        state["finalizing_segment"] = group["index"]
                        atomic_json(output / "status.json", state)
                        if time.monotonic() >= limit:
                            raise RuntimeError("independent verifier exceeded240s; native close will preserve all unverified evidence")
            state["completed_segments"] = len(completed)
            state["verification_backlog"] = 0
            state["latest_verifications"] = completed[-4:]
            if completed and completed[-1]["phase"] == "source" and completed[-1]["keep_video"]:
                state["retention"]["final_small_source_clip_retained"] = True
            state["numeric_bytes"] = closed_numeric
            state["warnings"] = list(dict.fromkeys(state["warnings"] + [warning for item in completed for warning in item["warnings"]]))[-30:]
            state["state"] = "completed" if not state["warnings"] else "completed_with_warnings"
            state["stop_reason"] = stop_reason
    except Exception as error:
        state["state"] = "failed"; state["error"] = str(error); state["stop_reason"] = "failure; no tracker reset/restart attempted"
        state["updated_utc"] = utc_now()
        atomic_json(output / "status.json", state)
    finally:
        if bridge:
            bridge.finish(identities)
            state["application_exit_code"] = bridge.process.returncode
        if probe:
            try:
                state["wire_final"] = probe.finish()
            except Exception as error:
                state["warnings"].append(f"subscriber shutdown: {error}")
        executor.shutdown(wait=True)
        close_inhibitor(inhibitor)
        telemetry.close()
        state["updated_utc"] = utc_now(); state["finished_utc"] = utc_now()
        atomic_json(output / "status.json", state)
        atomic_json(output / "report.json", state)
    return 1 if state["state"] == "failed" else 0


def read_status(output):
    state = json.loads((output / "status.json").read_text())
    age = (datetime.now(timezone.utc) - datetime.fromisoformat(state["updated_utc"])).total_seconds()
    terminal = state["state"] in ("completed", "completed_with_warnings", "failed")
    checks = {"heartbeat_age_seconds": age, "heartbeat_fresh": age <= 90,
              "runner_identity_matches": pid_identity(state["runner"]["pid"]) == state["runner"],
              "application_identity_matches": bool(state.get("application")) and pid_identity(state["application"]["pid"]) == state["application"]}
    operational = state["state"] == "running" and checks["heartbeat_fresh"] and checks["runner_identity_matches"] and checks["application_identity_matches"]
    healthy = operational and not state.get("warnings")
    return {"healthy_running": healthy, "operational_running": operational, "terminal": terminal,
            "checks": checks, "status": state,
            "scope": "Checks actual host PID start ticks and fresh heartbeat; a restricted PID namespace may correctly report unknown/unhealthy. Use host scope, not a sandbox namespace, to inspect the live run."}


def status_summary(result):
    state = result["status"]
    latest = state.get("latest_verifications", [])
    return {"healthy_running": result["healthy_running"], "operational_running": result["operational_running"],
            "terminal": result["terminal"], "checks": result["checks"], "state": state["state"],
            "updated_utc": state["updated_utc"], "deadline": state.get("deadline"),
            "runner": state["runner"], "application": state.get("application"),
            "binary_sha256": state["binary_sha256"], "processed_fps": state.get("processed_fps"),
            "application_cpu_percent_one_core": state.get("application_cpu_percent_one_core"),
            "process": state.get("process"), "storage": state.get("storage"),
            "completed_segments": state.get("completed_segments"), "verification_backlog": state.get("verification_backlog"),
            "latest_quality": [{"segment": entry["index"], "phase": entry["phase"], "verified": entry["verified"],
                                "sessions": [{"session": item["session"], "verified": item["verified"],
                                              "trajectory": item.get("trajectory"), "error": item.get("error"),
                                              "all_requested_video_complete": item.get("recording", {}).get("all_requested_video_complete"),
                                              "wire_exact": item.get("wire", {}).get("exact_in_order_without_duplicates")}
                                             for item in entry["sessions"]]}
                               for entry in latest[-1:]],
            "warnings": state.get("warnings", []), "error": state.get("error"),
            "stop_reason": state.get("stop_reason"), "sleep_inhibition": state.get("sleep_inhibition"),
            "scope": result["scope"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("run", "start"):
        command = commands.add_parser(name)
        command.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
        command.add_argument("--configs", type=Path, nargs="+", default=[Path(f"VR{i}.txt") for i in range(1, 5)])
        command.add_argument("--output", type=Path, required=True)
        command.add_argument("--until", default="2026-10-27T09:00:00+01:00")
        command.add_argument("--duration-seconds", type=float)
        command.add_argument("--segment-seconds", type=float, default=60)
        command.add_argument("--debug-probe-seconds", type=float, default=3)
        command.add_argument("--poll-seconds", type=float, default=10)
        command.add_argument("--display-fps", type=int, default=15)
        command.add_argument("--wire-port-base", type=int, default=21971)
        command.add_argument("--no-wire", dest="wire", action="store_false", default=True)
        command.add_argument("--no-inhibit", dest="inhibit", action="store_false", default=True)
        command.add_argument("--fixture-video", type=Path)
        command.add_argument("--fixture-fps", type=float, default=30)
        command.add_argument("--prepared", action="store_true", help=argparse.SUPPRESS)
    status = commands.add_parser("status"); status.add_argument("--output", type=Path, required=True)
    status.add_argument("--write-check", action="store_true")
    status.add_argument("--summary", action="store_true")
    stop = commands.add_parser("stop"); stop.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "status":
        result = read_status(args.output.resolve())
        if args.write_check:
            target = args.output / "hourly-checks"; target.mkdir(exist_ok=True)
            atomic_json(target / (datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ") + ".json"), result)
        print(json.dumps(status_summary(result) if args.summary else result, indent=2))
        return 0 if result["healthy_running"] or result["status"]["state"] == "completed" else 2
    if args.command == "stop":
        atomic_json(args.output / "stop.request", {"requested_utc": utc_now(), "request": "graceful finalization"})
        print("Graceful stop requested; poll status for completion."); return 0
    if not 1 <= len(args.configs) <= 4 or not 1 <= args.display_fps <= 60:
        parser.error("use1–4 configs and display FPS1–60")
    if any(not math.isfinite(value) or value <= 0 for value in [args.segment_seconds, args.debug_probe_seconds, args.poll_seconds, args.fixture_fps]):
        parser.error("all periods and fixture FPS must be positive")
    if not 1024 <= args.wire_port_base <= 65535 - len(args.configs) + 1:
        parser.error("wire ports must remain in1024–65535")
    args.binary = args.binary.resolve(); args.configs = [path.resolve() for path in args.configs]
    args.output = args.output.resolve()
    if args.fixture_video:
        args.fixture_video = args.fixture_video.resolve()
    if args.command == "start":
        args.output.mkdir(parents=True, exist_ok=False, mode=0o700)
        atomic_json(args.output / "launch.json", {"prepared_utc": utc_now(), "binary": str(args.binary), "configs": [str(p) for p in args.configs]})
        forwarded = [str(value) for value in sys.argv[2:]]
        # Relative file arguments are resolved by retaining the caller's cwd.
        log = (args.output / "supervisor.log").open("w")
        process = subprocess.Popen([sys.executable, str(Path(__file__).resolve()), "run", *forwarded, "--prepared"],
                                   stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        log.close()
        print(json.dumps({"queued": True, "runner": pid_identity(process.pid), "output": str(args.output),
                          "completion": "Poll status until healthy_running=true; queued launch is not proof of capture/recording."}, indent=2))
        return 0
    return run_soak(args)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as error:
        print(json.dumps({"ok": False, "error": str(error)}), file=sys.stderr)
        raise SystemExit(1)
