#!/usr/bin/env python3
"""Optional macOS acceptance test against unchanged published MATREX players.

Python only launches processes and checks their recordings. The wire publisher is
the Rust matrex_legacy_fixture example using production OutputSinks. This script
does not rebuild Unity, replace a DLL, inject a runtime script or republish poses.
Normal experiment JSON is changed only in newly extracted test copies.
"""
from __future__ import annotations

import argparse
import csv
import gzip
import hashlib
import io
import json
import math
from pathlib import Path
import platform
import shutil
import subprocess
import time
import zipfile

RELEASES = {
    "0.1.0": {
        "sha256": "49d3d68d80a17dfed72163aff731a995c6b9a7c4f3637de903187b67bc441b81",
        "source": "779926a80b02190d6dd0410f260e52bba105a04e",
    },
    "0.3.0": {
        "sha256": "49a67e383ad561ea668dbbbf5f8e0c0767b3d79ad91b1056be8bb164dae5b00e",
        "source": "cd207862cacf0a82d5e2ecfe39e23478679d5792",
    },
}
OLD_NETMQ_SHA256 = "ce9b347a4cf35c4ed96e1343f3ce84ce6514be680347e59de0f5a29f6ee6fba1"


def sha(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def protected_runtime(app):
    """Fingerprint executable/managed engine bytes, not writable experiment JSON."""
    return {
        str(p.relative_to(app)): sha(p)
        for p in sorted(app.rglob("*"))
        if p.is_file() and (
            "MacOS" in p.parts or p.suffix.lower() in (".dll", ".dylib", ".so")
        )
    }


def terminate_owned(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def recorded_csv(root):
    seen = set()
    for path in sorted(root.rglob("*")):
        if path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                for name in archive.namelist():
                    if name.endswith((".csv", ".csv.gz")):
                        raw = archive.read(name)
                        if name.endswith(".gz"):
                            raw = gzip.decompress(raw)
                        digest = hashlib.sha256(raw).hexdigest()
                        if digest not in seen:
                            seen.add(digest)
                            yield f"{path}:{name}", raw
        elif path.name.endswith((".csv", ".csv.gz")):
            raw = path.read_bytes()
            if path.suffix == ".gz":
                raw = gzip.decompress(raw)
            digest = hashlib.sha256(raw).hexdigest()
            if digest not in seen:
                seen.add(digest)
                yield str(path), raw


def analyze(case, cycles, radius):
    expected = [json.loads(line) for line in (case / "publisher/rows.jsonl").read_text().splitlines()]
    expected = [e for e in expected if e["publisher_active"] and e["row"]["accepted"]]
    observed, unmatched, paths = [], [], []
    groups = {}
    for name, raw in recorded_csv(case / "player"):
        reader = csv.DictReader(io.StringIO(raw.decode("utf-8-sig")))
        if not {"SensPosX", "SensPosY", "SensRotY", "GameObjectPosX", "GameObjectRotY"}.issubset(reader.fieldnames or []):
            continue
        paths.append(name)
        for row in reader:
            if not row.get("SensPosX"):
                continue
            values = {key: float(row[key]) for key in (
                "SensPosX", "SensPosY", "SensPosZ", "SensRotX", "SensRotY", "SensRotZ",
                "GameObjectPosX", "GameObjectPosY", "GameObjectPosZ", "GameObjectRotY",
            )}
            if not all(math.isfinite(v) for v in values.values()):
                raise AssertionError(f"Nonfinite player recording: {name}")
            x, y = values["SensPosX"], values["SensPosY"]
            if abs(x) + abs(y) < 1e-9:
                continue  # The original player's explicit not-yet-received pose.
            nearest = min(expected, key=lambda e: math.hypot(x-e["row"]["position"][0], y-e["row"]["position"][1]))
            error = math.hypot(x-nearest["row"]["position"][0], y-nearest["row"]["position"][1])
            yaw = math.degrees(nearest["row"]["heading"]) % 360
            yaw_error = abs((values["SensRotY"]-yaw+180) % 360-180)
            good = error < 0.0001 and yaw_error < 0.002 and max(abs(values[k]) for k in ("SensPosZ", "SensRotX", "SensRotZ")) < 0.002
            sample = {"position_error_sphere_rad": error, "yaw_error_degrees": yaw_error,
                      "publisher_seconds": nearest["seconds"], "values": values}
            if not good:
                unmatched.append(sample)
            observed.append(sample)
            key = (row["VR"], row["CurrentStep"])
            groups.setdefault(key, []).append(sample)
    movement = []
    for (rig, step), samples in sorted(groups.items()):
        # Discard the first quarter to exclude the historical initialization delay.
        samples = samples[len(samples)//4:]
        if len(samples) < 4:
            continue
        start, finish = samples[0]["values"], samples[-1]["values"]
        raw_distance = math.hypot(finish["SensPosX"]-start["SensPosX"], finish["SensPosY"]-start["SensPosY"])
        world_distance = math.hypot(finish["GameObjectPosX"]-start["GameObjectPosX"], finish["GameObjectPosZ"]-start["GameObjectPosZ"])
        sensor_yaw = (finish["SensRotY"]-start["SensRotY"]+180) % 360-180
        world_yaw = (finish["GameObjectRotY"]-start["GameObjectRotY"]+180) % 360-180
        movement.append({"rig": rig, "step": step, "rows": len(samples), "sensor_distance": raw_distance,
                         "world_distance": world_distance, "radius_ratio": world_distance/raw_distance if raw_distance else None,
                         "sensor_yaw_delta": sensor_yaw, "world_yaw_delta": world_yaw,
                         "movement_passed": raw_distance > 0.05 and abs(world_distance/raw_distance-radius) < radius*0.10,
                         "heading_passed": abs(sensor_yaw) > 0.5 and abs(world_yaw-sensor_yaw) < 0.5})
    log = (case / "unity.log").read_text(errors="replace")
    critical = [line for line in log.splitlines() if any(term in line for term in (
        "Unhandled ZMQ listener exception", "ZMQ listener failed", "Invalid tracking packet",
        "NullReferenceException", "MissingMethodException", "did not stop within",
    ))]
    coverage = {rig: sorted({int(step) for r, step in groups if r == rig}) for rig, _ in groups}
    gap_end = next(json.loads(line)["seconds"] for line in (case / "publisher/events.jsonl").read_text().splitlines()
                   if json.loads(line)["event"] == "publisher_rebound")
    reconnect = sum(s["publisher_seconds"] > gap_end + 0.5 for s in observed) > 20
    checks = {
        "received_measurements": len(observed) > 100,
        "wire_units_axes_and_heading": not unmatched and bool(observed),
        "rejected_marker_never_received": all(abs(s["values"]["SensPosX"]) < 100 for s in observed),
        "publisher_reconnect": reconnect,
        "all_rigs_all_scene_steps": len(coverage) == 4 and all(len(steps) >= cycles for steps in coverage.values()),
        "movement_every_scene": len(movement) >= 4*cycles and all(m["movement_passed"] for m in movement),
        "heading_every_scene": len(movement) >= 4*cycles and all(m["heading_passed"] for m in movement),
        "no_critical_receiver_errors": not critical,
    }
    functional = all(value for key, value in checks.items() if key != "no_critical_receiver_errors")
    return {"passed": all(checks.values()), "functional_compatibility_passed": functional,
            "clean_receiver_lifecycle": not critical, "checks": checks, "observations": len(observed), "unmatched": unmatched[:8],
            "max_position_error": max((s["position_error_sphere_rad"] for s in observed), default=None),
            "max_yaw_error_degrees": max((s["yaw_error_degrees"] for s in observed), default=None),
            "scene_coverage": coverage, "movement": movement, "critical_receiver_errors": critical,
            "recordings": paths}


def run_case(args, version, framing, port):
    case = args.output / f"v{version}-{framing}"
    case.mkdir()
    archive = args.v010_archive if version == "0.1.0" else args.v030_archive
    if sha(archive) != RELEASES[version]["sha256"]:
        raise ValueError(f"{archive} differs from the unchanged published archive")
    player_root = case / "player"
    player_root.mkdir()
    subprocess.run(["/usr/bin/ditto", "-x", "-k", str(archive), str(player_root)], check=True)
    app = player_root / f"MATREX-VR-{version}-macOS-universal/MATREX VR.app"
    runtime_before = protected_runtime(app)
    (case / "runtime-sha256-before.json").write_text(json.dumps(runtime_before, indent=2))
    managed = app / "Contents/Resources/Data/Managed"
    if sha(managed / "NetMQ.dll") != OLD_NETMQ_SHA256:
        raise AssertionError("Player does not carry the required unchanged old NetMQ DLL")
    stream = app / "Contents/Resources/Data/StreamingAssets"
    # 0.1 has no profile override. Both players receive only normal config edits
    # inside their own fresh copy, preserving every compiled runtime byte.
    system = json.loads((stream / "system_config.json").read_text())
    system["telemetry"] = {"enabled": False}
    radius = 1.3
    for rig in system["configs"]:
        rig.update(zmqAddress="127.0.0.1", zmqPort=port, closedLoopMode="FicTrac", sphereDiameter=2*radius)
        rig.setdefault("manualControls", {})["enabled"] = False
    (stream / "system_config.json").write_text(json.dumps(system, indent=2))
    experiment = json.loads((stream / "Templates/swarm.template.json").read_text())
    experiment.update(numberOfLocusts=0, agentCount=0, locustSpeed=0, agentSpeed=0,
                      autopilotEnabled=False, closedLoopPosition=1, closedLoopOrientation=1,
                      randomInitialRotation=False, resetPositionOnStart=True, resetRotationOnStart=True)
    experiment["vrConfigs"] = [{"vrIndex": i, "initialPosition": {"x": 0, "y": 1, "z": 0},
                                "initialRotation": {"x": 0, "y": 0, "z": 0}} for i in range(1, 5)]
    (stream / "matrex-wire-fixture.json").write_text(json.dumps(experiment, indent=2))
    sequence = {"autoStart": True, "loop": False, "randomise": False,
                "sequences": [{"sceneName": "Swarm", "duration": args.scene_seconds, "reloadScene": True,
                               "parameters": {"configFile": "matrex-wire-fixture.json"}} for _ in range(args.cycles)]}
    (stream / "sequenceConfig.json").write_text(json.dumps(sequence, indent=2))
    (case / "normal-test-config.json").write_text(json.dumps({"system": system, "experiment": experiment, "sequence": sequence}, indent=2))
    fixture_seconds = args.cycles*args.scene_seconds + 8
    publisher = subprocess.Popen([str(args.publisher), str(port), framing, str(fixture_seconds), str(case / "publisher")],
                                 stdout=(case / "publisher.stdout").open("w"), stderr=(case / "publisher.stderr").open("w"))
    player = None
    try:
        time.sleep(0.3)
        command = [str(app / "Contents/MacOS/MATREX VR"), "-batchmode", "-nographics", "-logFile", str(case / "unity.log")]
        if version == "0.3.0":
            command += ["--config-dir", str(stream), "--data-dir", str(player_root / "RunData")]
        (case / "launch.json").write_text(json.dumps({"command": command, "publisher": str(args.publisher),
            "publisher_sha256": sha(args.publisher), "unchanged_archive_sha256": sha(archive),
            "source_revision": RELEASES[version]["source"], "netmq_sha256": OLD_NETMQ_SHA256,
            "normal_json_edits_only": True, "synthetic": True, "started_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}, indent=2))
        player = subprocess.Popen(command, stdout=(case / "player.stdout").open("w"), stderr=(case / "player.stderr").open("w"))
        player.wait(timeout=fixture_seconds+12)
        publisher.wait(timeout=15)
        result = analyze(case, args.cycles, radius)
        runtime_after = protected_runtime(app)
        (case / "runtime-sha256-after.json").write_text(json.dumps(runtime_after, indent=2))
        result.update(version=version, framing=framing, player_exit=player.returncode, publisher_exit=publisher.returncode,
                      compiled_runtime_unchanged=runtime_after == runtime_before)
        result["passed"] = result["passed"] and player.returncode == 0 and publisher.returncode == 0 and result["compiled_runtime_unchanged"]
        result["functional_compatibility_passed"] = result["functional_compatibility_passed"] and player.returncode == 0 and publisher.returncode == 0 and result["compiled_runtime_unchanged"]
        (case / "result.json").write_text(json.dumps(result, indent=2))
        return result
    finally:
        if player is not None:
            terminate_owned(player)
        terminate_owned(publisher)
        # Retain raw recordings, configs, logs and cryptographic runtime proof;
        # only the new disposable extraction is removed to bound disk usage.
        recordings = case / "recordings"
        recordings.mkdir(exist_ok=True)
        index = []
        for name, raw in recorded_csv(player_root):
            reader = csv.DictReader(io.StringIO(raw.decode("utf-8-sig")))
            if "SensPosX" not in (reader.fieldnames or []):
                continue
            key = hashlib.sha256(raw).hexdigest()
            with gzip.open(recordings / f"{key}.csv.gz", "wb") as output:
                output.write(raw)
            index.append({"original": name, "uncompressed_sha256": key, "retained": f"{key}.csv.gz"})
        (recordings / "index.json").write_text(json.dumps(index, indent=2))
        if not args.keep_players:
            shutil.rmtree(player_root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--publisher", type=Path, required=True)
    parser.add_argument("--v010-archive", type=Path, required=True)
    parser.add_argument("--v030-archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cycles", type=int, default=6)
    parser.add_argument("--scene-seconds", type=float, default=3)
    parser.add_argument("--base-port", type=int, default=29871)
    parser.add_argument("--only", choices=("0.1.0-single", "0.1.0-topic_json", "0.3.0-single", "0.3.0-topic_json"))
    parser.add_argument("--keep-players", action="store_true", help="Retain the large disposable player extractions; raw recordings and runtime hashes are always retained")
    args = parser.parse_args()
    if platform.system() != "Darwin":
        parser.error("The pinned test archives are macOS universal players")
    if not 2 <= args.cycles <= 50 or not 1 <= args.scene_seconds <= 10:
        parser.error("Use 2..50 cycles and 1..10 seconds per scene")
    if args.cycles*args.scene_seconds+8 > 300 or not 1 <= args.base_port <= 65532:
        parser.error("The complete fixture must fit 300 seconds and four valid TCP ports")
    args.publisher = args.publisher.resolve()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    results = []
    for version in RELEASES:
        for framing in ("single", "topic_json"):
            if args.only and args.only != f"{version}-{framing}":
                continue
            result = run_case(args, version, framing, args.base_port+len(results))
            results.append(result)
            print(json.dumps({"version": version, "framing": framing, "passed": result["passed"], "checks": result["checks"]}), flush=True)
    report = {"passed": all(r["passed"] for r in results), "functional_compatibility_passed": all(r["functional_compatibility_passed"] for r in results),
              "platform": platform.platform(), "cases": results,
              "scope": "Unchanged published macOS players; normal JSON config only; synthetic Rust OutputSinks fixture, no Python republisher. No physical-camera/display or Windows/Linux player claim."}
    (args.output / "results.json").write_text(json.dumps(report, indent=2))
    raise SystemExit(0 if report["passed"] else 1)


if __name__ == "__main__":
    main()
