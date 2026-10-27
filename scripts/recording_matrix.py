"""Measure real native camera recording modes serially, then verify their media.

Development analysis only. Native application CPU/RSS and processed frame rates
are measured for data-only, raw-only, debug-only and both-video recording. Warmup,
startup, recorder finalization, Python/control sampling and independent ffprobe
verification are outside the sustained application measurement. Source geometry
and camera settings come from config copies; source files are never edited.

Linux /proc CPU ticks provide process CPU time. The metric is one-core percent
(400% means four full CPU cores), not percent of total host capacity or GPU use.
Live content differs between cases: complete quality rows are retained, but this
matrix does not establish physical accuracy or identical-image tracking parity.
V4L capture sequence counters do not expose verified sensor-level frame loss.
"""
import argparse
import csv
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import statistics
import subprocess
import shutil
import time

from benchmark_vr3 import dat_rows
from test_native_rigs import NativeBridge, field, frame_progress
from native_zmq_probe import NativeWireProbe

OPTIONS = {"data": {"raw": False, "debug": False},
           "raw": {"raw": True, "debug": False},
           "debug": {"raw": False, "debug": True},
           "both": {"raw": True, "debug": True}}


def process_metrics(pid):
    root = Path("/proc") / str(pid)
    # /proc/stat's comm may contain spaces or parentheses. The final ')' ends it.
    fields = (root / "stat").read_text().rsplit(")", 1)[1].split()
    ticks = os.sysconf("SC_CLK_TCK")
    status = dict(line.split(":", 1) for line in (root / "status").read_text().splitlines() if ":" in line)
    return {"cpu_seconds": (int(fields[11]) + int(fields[12])) / ticks,
            "rss_bytes": int(status["VmRSS"].split()[0]) * 1024,
            "threads": int(status["Threads"])}


def clock_diagnostics(rows, nominal_fps=100.0):
    nominal = 1000.0 / nominal_fps
    intervals = [right[21] - left[21] for left, right in zip(rows, rows[1:])]
    sorted_intervals = sorted(intervals)
    first_second = [row for row in rows if row[21] - rows[0][21] <= 1000.0]
    first_intervals = [right[21] - left[21] for left, right in zip(first_second, first_second[1:])]
    return {"nominal_interval_ms": nominal, "intervals": len(intervals),
            "median_interval_ms": statistics.median(intervals) if intervals else None,
            "p95_interval_ms": sorted_intervals[min(len(intervals)-1, math.ceil(len(intervals)*.95)-1)] if intervals else None,
            "maximum_interval_ms": max(intervals, default=None),
            "intervals_above_1_5x_nominal": sum(value > 1.5 * nominal for value in intervals),
            "intervals_above_1_5x_nominal_per_1000_intervals": 1000 * sum(value > 1.5 * nominal for value in intervals) / max(1, len(intervals)),
            "intervals_above_2_5x_nominal": sum(value > 2.5 * nominal for value in intervals),
            "nominal_missing_interval_estimate": sum(max(0, round(value / nominal)-1) for value in intervals),
            "first_source_clock_second": {"rows": len(first_second),
                "maximum_interval_ms": max(first_intervals, default=None),
                "intervals_above_1_5x_nominal": sum(value > 1.5*nominal for value in first_intervals)},
            "boundary": "Actual emitted source-clock gaps, including startup and overload transitions, from complete numeric rows. Rounded nominal missing intervals are diagnostics only; no verified sensor sequence counter or sensor-level zero-loss claim."}


def verify_live_queue_samples(case):
    samples = [case["recording_started"], case["before"], case["after"], case["after_record_stop"]]
    samples.extend(sample["sessions"] for sample in case["startup_transition_samples"])
    samples.extend(sample["sessions"] for sample in case["samples"])
    seen = 0
    for sessions in samples:
        for session in sessions:
            media = session.get("media_recording")
            if media is None:
                continue
            for kind in ("raw", "debug"):
                stream = media[kind]
                if not stream["requested"]:
                    continue
                if not 0 <= stream["encoded_frames"] <= stream["submitted_frames"] <= media["numeric_frames"]:
                    raise RuntimeError(f"live {kind} actor counts contradict complete numeric count: {media}")
                if stream["pending_frames"] > stream["queue_capacity"] + 1 or stream["pending_bytes"] > stream["queue_byte_capacity"]:
                    raise RuntimeError(f"live {kind} actor exceeded bounded queue+inflight storage: {stream}")
                seen += 1
    return {"stream_snapshots_checked": seen, "queue_and_inflight_bounds_preserved": True,
            "boundary": "Status samples may miss brief peaks; source code/unit coverage establishes bounds between samples. Queue frame limit excludes one in-flight encoder frame; byte limit includes it."}


def probe_video(path):
    probe = subprocess.run([
        "ffprobe", "-v", "error", "-count_frames", "-select_streams", "v:0",
        "-show_entries", "stream=codec_name,width,height,r_frame_rate,nb_read_frames",
        "-of", "json", str(path),
    ], capture_output=True, text=True, check=True, timeout=120)
    if probe.stderr.strip():
        raise RuntimeError(f"independent media decode reported errors for {path}: {probe.stderr}")
    streams = json.loads(probe.stdout)["streams"]
    if len(streams) != 1 or "nb_read_frames" not in streams[0]:
        raise RuntimeError(f"independent decode returned no finite video frame count: {path}")
    return streams[0]


def verify_recording(directory, options, *, allow_partial=False, discard_verified_video=False,
                     expected_codec=None, expected_encoding=None, use_saved_verification=False):
    quality_paths = list(directory.glob("*-quality.csv"))
    all_paths = list(directory.glob("*-all-frames.dat"))
    accepted_paths = [p for p in directory.glob("*.dat") if not p.name.endswith("-all-frames.dat")]
    if len(quality_paths) != 1 or len(all_paths) != 1 or len(accepted_paths) != 1:
        raise RuntimeError(f"expected one numeric/quality epoch in {directory}")
    with quality_paths[0].open(newline="") as stream:
        quality = list(csv.DictReader(stream))
    rows = dat_rows(all_paths[0])
    accepted = dat_rows(accepted_paths[0])
    if not rows or len(quality) != len(rows):
        raise RuntimeError(f"missing or unaligned numeric/quality evidence in {directory}")
    if any(int(decision["frame"]) != int(row[0])
           or abs(float(decision["timestamp_ms"]) - row[21]) > 1e-5
           for row, decision in zip(rows, quality)):
        raise RuntimeError(f"numeric/quality frame alignment failed in {directory}")
    if any(rows[i][0] != rows[i - 1][0] + 1 or rows[i][21] <= rows[i - 1][21]
           for i in range(1, len(rows))):
        raise RuntimeError(f"nonconsecutive numeric frames or clocks in {directory}")
    expected_accepted = [row for row, decision in zip(rows, quality) if decision["accepted"] == "true"]
    if accepted != expected_accepted:
        raise RuntimeError(f"accepted-only data differs from full quality decisions in {directory}")
    media = []
    manifests = list(directory.glob("*-media/media-status.json"))
    if len(manifests) > 1:
        raise RuntimeError(f"multiple media manifests in {directory}")
    manifest = json.loads(manifests[0].read_text()) if manifests else None
    if allow_partial and any(options.values()) and manifest is None:
        raise RuntimeError(f"partial-video policy requires explicit media-status.json in {directory}")
    if manifest is not None and manifest["numeric_frames"] != len(rows):
        raise RuntimeError(f"media manifest numeric frame count differs from complete quality rows in {directory}")
    if manifest is not None and expected_codec is not None and manifest["codec"] != expected_codec:
        raise RuntimeError(f"media manifest codec differs from explicit selection in {directory}")
    if manifest is not None and expected_encoding is not None and manifest["encoding"] != expected_encoding:
        raise RuntimeError(f"media manifest tuning differs from explicit selection in {directory}")
    for kind in ("raw", "debug"):
        files = list(directory.glob(f"*-media/{kind}.*"))
        saved = directory / "verified-media-excerpts" / f"{kind}-verification.json"
        if not files and options[kind] and use_saved_verification and saved.exists():
            prior = json.loads(saved.read_text())
            if (prior["all_frame_numeric_rows"] != len(rows) or prior["quality_rows"] != len(quality)
                    or prior["media_manifest"] != manifest
                    or not prior["verified_media"]["video_removed_after_verification"]):
                raise RuntimeError(f"saved {kind} verification contradicts retained numeric/manifest evidence")
            value = prior["verified_media"]
            if not allow_partial and not value["complete_video_aligned_to_all_numeric_rows"]:
                raise RuntimeError(f"saved {kind} verification was an incomplete prefix")
            value["verification_loaded_from_saved_pre_deletion_evidence"] = True
            media.append(value)
            continue
        if bool(files) != options[kind] or len(files) > 1:
            raise RuntimeError(f"{kind} recording selection differs from request in {directory}")
        if not files:
            if manifest is not None and (manifest[kind]["requested"] or manifest[kind]["state"] != "disabled"):
                raise RuntimeError(f"unselected {kind} incorrectly reported by media manifest")
            continue
        path = files[0]
        stream = probe_video(path)
        codec = manifest["codec"] if manifest is not None else expected_codec
        decoded_codec = {"h264": "h264", "h264_mp4": "h264", "mjpg": "mjpeg",
                         "raw": "rawvideo", "xvid": "mpeg4", "mpg4": "mpeg4"}.get(codec)
        if decoded_codec is not None and stream["codec_name"] != decoded_codec:
            raise RuntimeError(f"actual decoded {kind} codec differs from requested {codec}: {stream}")
        count = int(stream["nb_read_frames"])
        state = manifest[kind] if manifest is not None else None
        complete = state is None or state["state"] == "complete"
        if state is not None:
            if not state["requested"] or state["pending_frames"] != 0:
                raise RuntimeError(f"{kind} final media state is not drained/requested in {directory}: {state}")
            if state["pending_bytes"] != 0:
                raise RuntimeError(f"{kind} final media state retains pending byte payloads")
            if state["encoded_frames"] != count:
                raise RuntimeError(f"{kind} manifest encoded count differs from independent decode in {directory}")
            if not state["container_finalized"]:
                raise RuntimeError(f"{kind} did not finalize a valid complete/prefix container")
            submitted = state["submitted_frames"]
            if not 0 < count <= submitted <= len(rows):
                raise RuntimeError(f"{kind} encoded/submitted/numeric bounds are invalid")
            expected_prefix = {
                "first_source_frame": rows[0][0], "last_source_frame": rows[submitted-1][0],
                "first_encoded_source_frame": rows[0][0], "last_encoded_source_frame": rows[count-1][0],
                "first_timestamp_ms": rows[0][21], "last_timestamp_ms": rows[submitted-1][21],
                "first_encoded_timestamp_ms": rows[0][21], "last_encoded_timestamp_ms": rows[count-1][21]}
            if any(state[key] is None or abs(state[key]-expected) > 1e-5 for key, expected in expected_prefix.items()):
                raise RuntimeError(f"{kind} media prefix source identities differ from numeric rows: {state}")
        if complete:
            if path.suffix == ".partial" or count != len(rows):
                raise RuntimeError(f"completed {kind} decoded frame count differs from all numeric rows in {directory}")
            if state is not None and state["submitted_frames"] != len(rows):
                raise RuntimeError(f"completed {kind} manifest submission count differs from numeric rows")
        else:
            if not allow_partial or state["state"] != "stopped_overload":
                raise RuntimeError(f"{kind} did not complete; partial output is not permitted: {state}")
            final = Path(state["output_path"])
            partial = Path(state["partial_path"])
            if not final.is_absolute():
                final = manifests[0].parent / final
            if not partial.is_absolute():
                partial = manifests[0].parent / partial
            if final.exists() or path.resolve() != partial.resolve() or path.suffix != ".partial":
                raise RuntimeError(f"{kind} overload could be mistaken for a complete video: {state}")
            if not state["warning"] or state["submitted_frames"] != count or not 0 < count < len(rows):
                raise RuntimeError(f"{kind} overload has no explicit finite encoded prefix/warning: {state}")
        expected_size = (320, 240) if kind == "raw" else (640, 480)
        if (stream["width"], stream["height"]) != expected_size:
            raise RuntimeError(f"unexpected {kind} dimensions in {path}: {stream}")
        digest = hashlib.sha256()
        with path.open("rb") as contents:
            for block in iter(lambda: contents.read(1024 * 1024), b""):
                digest.update(block)
        media.append({"kind": kind, "path": str(path), "bytes": path.stat().st_size,
                      "sha256": digest.hexdigest(), "independent_full_decode": stream,
                      "complete_video_aligned_to_all_numeric_rows": complete,
                      "explicit_overload_prefix": not complete,
                      "manifest": state, "video_removed_after_verification": discard_verified_video})
        if discard_verified_video:
            excerpts = directory / "verified-media-excerpts"
            excerpts.mkdir(exist_ok=True)
            command = ["ffmpeg", "-v", "error", "-i", str(path), "-vf",
                       f"select='eq(n,0)+eq(n,{count-1})'", "-vsync", "0", "-frames:v", "2",
                       str(excerpts / f"{kind}-%02d.png")]
            subprocess.run(command, capture_output=True, text=True, check=True, timeout=120)
            media[-1]["retained_first_and_last_image_excerpts"] = [str(p) for p in sorted(excerpts.glob(f"{kind}-*.png"))]
            (excerpts / f"{kind}-verification.json").write_text(json.dumps({
                "all_frame_numeric_rows": len(rows), "quality_rows": len(quality),
                "first_numeric_frame": int(rows[0][0]), "last_numeric_frame": int(rows[-1][0]),
                "media_manifest": manifest, "verified_media": media[-1]}, indent=2) + "\n")
            path.unlink()
    errors = [float(decision["fit_error"]) for decision in quality]
    return {"directory": str(directory), "all_frame_rows": len(rows),
            "quality_rows": len(quality), "accepted_rows": len(accepted),
            "accepted_fraction": len(accepted) / len(rows),
            "first_frame": int(rows[0][0]), "last_frame": int(rows[-1][0]),
            "recorded_source_clock_span_seconds": (rows[-1][21] - rows[0][21]) / 1000,
            "capture_clock_diagnostics": clock_diagnostics(rows),
            "median_fit_error": statistics.median(errors),
            "numeric_quality_and_media_aligned": all(m["complete_video_aligned_to_all_numeric_rows"] for m in media),
            "numeric_quality_complete_and_media_state_verified": True,
            "all_requested_video_complete": all(m["complete_video_aligned_to_all_numeric_rows"] for m in media),
            "media_manifest": manifest, "media": media,
            "sensor_frame_loss_verified": False}


def run_group(args, count):
    output = args.output.resolve() / f"{count}-camera"
    output.mkdir(parents=True, exist_ok=False)
    copied = []
    ports = [args.wire_port_base + index for index in range(count)] if args.wire else []
    for index, original in enumerate(args.configs[:count], 1):
        text = original.read_text()
        for key, value in [("save_raw", "n"), ("save_debug", "n"), ("sock_port", "0"),
                           ("zmq_port", str(ports[index - 1]) if ports else "0"),
                           ("com_port", ""), ("do_display", "y"),
                           ("zmq_output_mode", "native_ft"),
                           ("vid_codec", args.codec)]:
            text = field(text, key, value)
        path = output / "configs" / f"VR{index}" / "config.txt"
        path.parent.mkdir(parents=True)
        path.write_text(text)
        copied.append(path)
    command = [str(args.binary.resolve()), "--config", str(copied[0]), "--windows", str(count),
               "--grid", "1x1" if count == 1 else "2x2", "--window-size", "480x360", "--control-stdio"]
    wire = NativeWireProbe(ports) if ports else None
    bridge = NativeBridge(command, output)
    report = {"command": command, "configs": [str(p.resolve()) for p in args.configs[:count]],
              "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              "camera_count": count, "codec": args.codec, "encoding_tuning": args.encoding,
              "display_target_fps": args.display_fps,
              "allow_explicit_overload_prefix": args.allow_partial, "wire_ports": ports, "cases": [],
              "platform": platform.platform(), "host_logical_cpus": os.cpu_count(),
              "display_environment": {key: os.environ.get(key) for key in
                                      ("DISPLAY", "WAYLAND_DISPLAY", "WINIT_UNIX_BACKEND", "WGPU_BACKEND")},
              "timing_and_quality_scope": __doc__}
    identities = []
    try:
        sessions = bridge.request("sessions")
        identities = [s["session"] for s in sessions]
        if len(identities) != count:
            raise RuntimeError(f"expected {count} native sessions")
        if args.display_fps is not None:
            for identity in identities:
                bridge.request("view", identity, display_fps=args.display_fps)
        for identity in identities:
            bridge.request("start", identity)
        ready = bridge.wait(lambda states: all(s["frames"] >= 10 and s["camera"] is not None for s in states))
        if any(s["camera"]["current_format"] != {"width": 320, "height": 240, "fps": 100.0} for s in ready):
            raise RuntimeError(f"matrix requires calibrated 320x240 at 100 fps: {ready}")
        report["startup"] = ready
        if args.display_fps is not None:
            bridge.wait(lambda states: all(s["preview_target_fps"] == args.display_fps for s in states))
        if wire:
            # Let real subscriptions/greetings settle before any evidence take.
            time.sleep(1)
        for mode in args.modes:
            selection = OPTIONS[mode]
            case = {"mode": mode, "selection": selection, "recordings": []}
            report["cases"].append(case)
            if args.encoding == "raw" or (args.encoding in (None, "configured") and args.codec == "raw"):
                bytes_per_frame = 3 * (320*240*selection["raw"] + 640*480*selection["debug"])
                estimate = math.ceil((args.seconds + args.warmup + 2) * 100 * count * bytes_per_frame)
                available = shutil.disk_usage(output).free
                case["raw_disk_preflight"] = {"free_bytes": available, "conservative_case_bytes": estimate,
                    "reserve_bytes": 1024*1024*1024,
                    "boundary": "Conservative requested100FPS RGB payload plus two seconds of startup/finalization margin; actual AVI headers add little. This prevents the test from deliberately exhausting the shared filesystem; it is not a production disk-full test."}
                if available < estimate + 1024*1024*1024:
                    raise RuntimeError("raw video test would consume the reserved filesystem space; shorten the case or clear only verified prior video artifacts")
            directories = {identity: output / mode / f"session-{identity}" for identity in identities}
            case["before_record_start"] = bridge.request("sessions")
            case["before_record_start_process"] = process_metrics(bridge.process.pid)
            requested = time.monotonic()
            for identity in identities:
                record_args = {"directory": str(directories[identity]), "record_options": selection}
                if args.encoding is not None:
                    record_args["recording_tuning"] = {"encoding": args.encoding}
                bridge.request("record_start", identity, **record_args)
            case["recording_started"] = bridge.wait(lambda states: all(s["recording"] and s["record_options"] == selection
                                           and s["recording_directory"] == str(directories[s["session"]])
                                           for s in states))
            case["start_recording_latency_seconds"] = time.monotonic() - requested
            case["startup_transition_samples"] = []
            warmup_started = time.monotonic()
            for target in (.1, .25, .5, 1.0, args.warmup):
                if target > args.warmup or target < time.monotonic() - warmup_started:
                    continue
                time.sleep(max(0, target - (time.monotonic() - warmup_started)))
                case["startup_transition_samples"].append({"elapsed_since_record_request_seconds": time.monotonic() - requested,
                    "sessions": bridge.request("sessions"), "process": process_metrics(bridge.process.pid)})
            before = bridge.request("sessions")
            start_metrics = process_metrics(bridge.process.pid)
            started = time.monotonic()
            samples = []
            while time.monotonic() - started < args.seconds:
                time.sleep(min(1, max(0, args.seconds - (time.monotonic() - started))))
                states = bridge.wait(lambda ss: True)
                samples.append({"elapsed_seconds": time.monotonic() - started,
                                "process": process_metrics(bridge.process.pid), "sessions": states})
            after = bridge.request("sessions")
            ended = time.monotonic()
            end_metrics = process_metrics(bridge.process.pid)
            elapsed = ended - started
            case.update({"elapsed_seconds": elapsed, "requested_seconds": args.seconds,
                         "warmup_seconds": args.warmup,
                         "application_cpu_seconds": end_metrics["cpu_seconds"] - start_metrics["cpu_seconds"],
                         "cpu_percent_one_core": 100 * (end_metrics["cpu_seconds"] - start_metrics["cpu_seconds"]) / elapsed,
                         "rss_median_bytes": statistics.median(s["process"]["rss_bytes"] for s in samples),
                         "rss_max_bytes": max(s["process"]["rss_bytes"] for s in samples),
                         "progress": frame_progress(before, after, elapsed),
                         "before": before, "after": after, "samples": samples})
            case["minimum_target_processed_fps"] = 95.0
            case["meets_95_percent_of_requested_100_fps"] = all(
                p["processed_frames_per_second"] >= 95.0 for p in case["progress"])
            if any(p["processed_frame_delta"] <= 0 for p in case["progress"]):
                raise RuntimeError(f"recording stopped a camera: {case['progress']}")
            stop = time.monotonic()
            for identity in identities:
                bridge.request("record_stop", identity)
            case["after_record_stop"] = bridge.wait(lambda states: all(not s["recording"] for s in states))
            case["stop_recording_and_finalize_latency_seconds"] = time.monotonic() - stop
            # Parse numeric evidence now; independent media decoding waits until
            # the native process closes so it cannot pollute another timed case.
            case["recording_directories"] = {str(k): str(v) for k, v in directories.items()}
        report["passed_capture_and_recording"] = True
        for identity in reversed(identities):
            bridge.request("close", identity, discard=True)
        bridge.process.wait(timeout=15)
        if bridge.process.returncode != 0:
            raise RuntimeError(f"native process exited {bridge.process.returncode}")
    except Exception as error:
        report["passed_capture_and_recording"] = False
        report["failure"] = str(error)
        raise
    finally:
        wire_error = None
        if wire:
            try:
                report["wire_load"] = wire.finish()
                report["whole_group_accepted_wire_clock_diagnostics"] = []
                for port in ports:
                    rows = [[float(value) for value in packet[4:].split(b",")] for packet in wire.packets[port]]
                    if not rows:
                        raise RuntimeError(f"no real accepted FT traffic received on port {port}")
                    item = clock_diagnostics(rows)
                    item["port"] = port
                    item["boundary"] = "Accepted-only FT traffic across the whole group, including record start/stop/finalization transitions. Rejected/recovering rows are intentionally absent from this wire contract, so these gaps are not all-frame capture-loss measurements. Per-take all-frame diagnostics are retained separately."
                    report["whole_group_accepted_wire_clock_diagnostics"].append(item)
            except Exception as error:
                wire_error = str(error)
                report["wire_failure"] = wire_error
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        bridge.finish(identities)
        if wire_error:
            raise RuntimeError("wire validation failed: " + wire_error)
    for case in report["cases"]:
        case["live_queue_validation"] = verify_live_queue_samples(case)
        if wire:
            case["wire_recording_agreement"] = wire.compare_recordings(
                [Path(p) for p in case["recording_directories"].values()], minimum_fraction=1.0)
    # Persist wire/queue proof before strict media verification can fail, and
    # retain each completed verification before deleting another large video.
    report["passed_media_verification"] = False
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    try:
        for case in report["cases"]:
            for directory in case["recording_directories"].values():
                case["recordings"].append(verify_recording(Path(directory), case["selection"],
                    allow_partial=args.allow_partial, discard_verified_video=args.discard_verified_video,
                    expected_codec={"mjpeg": "mjpg", "raw": "raw"}.get(args.encoding, args.codec),
                    expected_encoding=args.encoding))
                (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    except Exception as error:
        report["media_verification_failure"] = str(error)
        (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
        raise
    report["passed_media_verification"] = True
    report["passed_numeric_quality_and_media_alignment"] = all(
        r["numeric_quality_and_media_aligned"] for c in report["cases"] for r in c["recordings"])
    report["passed_numeric_quality_and_explicit_media_state"] = True
    report["all_requested_video_complete"] = all(
        r["all_requested_video_complete"] for c in report["cases"] for r in c["recordings"])
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/fictrac-desktop"))
    parser.add_argument("--configs", type=Path, nargs="+", default=[Path(f"VR{i}.txt") for i in range(1, 5)])
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--counts", type=int, nargs="+", choices=(1, 4), default=[1, 4])
    parser.add_argument("--modes", nargs="+", choices=tuple(OPTIONS), default=list(OPTIONS))
    parser.add_argument("--codec", choices=("h264", "mjpg", "raw", "xvid", "mpg4", "h264_mp4"), default="h264")
    parser.add_argument("--encoding", choices=("configured", "mjpeg", "raw"),
                        help="new transient native recording tuning; omit for legacy bridge")
    parser.add_argument("--display-fps", type=int, choices=range(1, 61),
                        help="set transient native preview target independently of capture/recording")
    parser.add_argument("--allow-partial", action="store_true",
                        help="verify explicit stopped_overload prefixes; never mark them complete")
    parser.add_argument("--require-fps", action="store_true", help="fail if any camera processes below 95 FPS")
    parser.add_argument("--expect-overload", action="store_true", help="require at least one stopped_overload stream")
    parser.add_argument("--wire", action="store_true", help="real FT subscriber plus slow/incomplete peers; require all accepted rows byte-exact")
    parser.add_argument("--wire-port-base", type=int, default=39140)
    parser.add_argument("--discard-verified-video", action="store_true",
                        help="remove only video bytes after full decode/count/hash verification; preserve evidence reports")
    parser.add_argument("--seconds", type=float, default=10)
    parser.add_argument("--warmup", type=float, default=2)
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("this precise CPU-tick recording matrix currently measures Linux only")
    if not math.isfinite(args.seconds) or not 2 <= args.seconds <= 300:
        parser.error("--seconds must be 2..300")
    if not math.isfinite(args.warmup) or not 0 <= args.warmup <= 60:
        parser.error("--warmup must be 0..60")
    if len(args.configs) < max(args.counts) or len(set(args.counts)) != len(args.counts):
        parser.error("provide enough distinct configs and no duplicate camera-count cases")
    if args.expect_overload and not args.allow_partial:
        parser.error("--expect-overload requires --allow-partial")
    if not 1024 <= args.wire_port_base <= 65531:
        parser.error("--wire-port-base must leave four valid nonprivileged ports")
    if args.wire and (args.seconds + args.warmup) * len(args.modes) > 65:
        parser.error("wire packet evidence capacity requires <=65 seconds per camera group")
    args.output.resolve().mkdir(parents=True, exist_ok=False)
    reports = [run_group(args, count) for count in args.counts]
    summary = [{"camera_count": report["camera_count"], "cases": [
        {"mode": case["mode"], "cpu_percent_one_core": case["cpu_percent_one_core"],
         "processed_frames_per_second": [p["processed_frames_per_second"] for p in case["progress"]],
         "meets_95_percent_of_requested_100_fps": case["meets_95_percent_of_requested_100_fps"],
         "numeric_quality_and_media_aligned": all(r["numeric_quality_and_media_aligned"] for r in case["recordings"]),
         "numeric_quality_complete_and_media_state_verified": all(r["numeric_quality_complete_and_media_state_verified"] for r in case["recordings"]),
         "all_requested_video_complete": all(r["all_requested_video_complete"] for r in case["recordings"]),
         "source_clock_gap_diagnostics": [r["capture_clock_diagnostics"] for r in case["recordings"]],
         "explicit_overload_streams": sum(m["explicit_overload_prefix"] for r in case["recordings"] for m in r["media"])}
        for case in report["cases"]]} for report in reports]
    (args.output.resolve() / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    if args.require_fps and not all(c["meets_95_percent_of_requested_100_fps"] for group in summary for c in group["cases"]):
        raise RuntimeError("at least one camera failed the declared 95 FPS processed-numeric target")
    if args.expect_overload and not any(c["explicit_overload_streams"] for group in summary for c in group["cases"]):
        raise RuntimeError("intentional overload case did not exercise explicit overload stopping")


if __name__ == "__main__":
    main()
