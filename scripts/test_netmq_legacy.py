#!/usr/bin/env python3
"""Test the Rust publisher against the exact shipped NetMQ 4.0.0.1 DLL.

Requires a .NET SDK and externally supplied historical NetMQ.dll/AsyncIO.dll.
Build the producer with:
  cargo build -p fictrac-io --example matrex_legacy_fixture --release --locked
Then pass --publisher target/release/examples/matrex_legacy_fixture and
--managed-directory PATH/TO/UNCHANGED/VR/Contents/Resources/Data/Managed.

Python only orchestrates; it never publishes or transforms live wire data. This
is a DLL transport oracle, not a substitute for unchanged VR-player acceptance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading

EXPECTED_NETMQ = "ce9b347a4cf35c4ed96e1343f3ce84ce6514be680347e59de0f5a29f6ee6fba1"
ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def expected_payload(row):
    # The independent Python reference reproduces the FT writer's 12 decimals,
    # then the historical republisher's float() and math.degrees() operations.
    return json.dumps(dict(x=float(f"{row['position'][0]:.12f}"),
                           y=float(f"{row['position'][1]:.12f}"), z=0.0,
                           yaw=math.degrees(float(f"{row['heading']:.12f}")),
                           pitch=0.0, roll=0.0))


def run_case(args, work, probe, mode, env):
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    receiver = subprocess.Popen([args.dotnet, str(probe), f"tcp://127.0.0.1:{port}", "8"],
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, env=env)
    lines, values = [], []
    ready = threading.Event()

    def read_output():
        for line in receiver.stdout:
            lines.append(line)
            try:
                value = json.loads(line)
                values.append(value)
                if value.get("kind") == "ready":
                    ready.set()
            except ValueError:
                pass

    reader = threading.Thread(target=read_output, daemon=True)
    reader.start()
    source = work / ("source-" + mode)
    try:
        if not ready.wait(15):
            raise RuntimeError("Old DLL subscriber did not become ready: " + "".join(lines))
        produced = subprocess.run([str(args.publisher.resolve()), str(port), mode, "6", str(source)],
                                  capture_output=True, text=True, timeout=20)
        if produced.returncode:
            raise RuntimeError("Rust publisher failed: " + produced.stderr)
        try:
            receiver.wait(timeout=12)
        except subprocess.TimeoutExpired:
            # This old .NET Framework DLL can hang in Cleanup on newer .NET.
            # Report that separately; never hide it as a successful cleanup test.
            receiver.kill()
            receiver.wait(timeout=5)
    finally:
        if receiver.poll() is None:
            receiver.kill()
            receiver.wait(timeout=5)
        reader.join(timeout=3)
    (work / (mode + "-netmq.jsonl")).write_text("".join(lines))
    records = [json.loads(line) for line in (source / "rows.jsonl").read_text().splitlines()]
    expected = {expected_payload(record["row"]): index for index, record in enumerate(records)
                if record["row"]["accepted"] and record["publisher_active"]}
    indices = []
    for value in values:
        if value.get("kind") != "message":
            continue
        frames = value["frames"]
        if mode == "single":
            if len(frames) != 1:
                raise RuntimeError("Single JSON must be exactly one UTF-8 frame")
        elif len(frames) != 2 or frames[0] != "FT":
            raise RuntimeError("Multipart must be exactly [FT, JSON]")
        if frames[-1] not in expected:
            raise RuntimeError("NetMQ payload differs from Python republisher: " + frames[-1])
        indices.append(expected[frames[-1]])
    if not indices or any(a >= b for a, b in zip(indices, indices[1:])):
        raise RuntimeError("Missing, duplicate or reordered NetMQ measurements")
    gap_end = 6 * .45 + 1.0
    if not any(records[index]["seconds"] < 6 * .45 for index in indices):
        raise RuntimeError("No measurement before publisher reconnect")
    if not any(records[index]["seconds"] >= gap_end for index in indices):
        raise RuntimeError("NetMQ did not reconnect to the rebound Rust publisher")
    return dict(framing=mode, received=len(indices), source_rows=len(records),
                first_index=indices[0], last_index=indices[-1], payloads_exact=True,
                reconnect=True, netmq_cleanup_returned=any(v.get("kind") == "cleanup_complete" for v in values),
                receiver_exit_code=receiver.returncode)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--publisher", required=True, type=Path)
    parser.add_argument("--managed-directory", required=True, type=Path)
    parser.add_argument("--dotnet", default="dotnet")
    parser.add_argument("--workdir", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    old = args.managed_directory / "NetMQ.dll"
    actual = digest(old)
    if actual != EXPECTED_NETMQ:
        raise SystemExit("Refusing a substituted DLL: expected shipped NetMQ 4.0.0.1 SHA256 " + EXPECTED_NETMQ)
    work = args.workdir or Path(tempfile.mkdtemp(prefix="hejjetrac-netmq-legacy-"))
    if args.workdir:
        work.mkdir(parents=True, exist_ok=False)
    source = work / "probe"
    shutil.copytree(ROOT / "scripts/netmq_legacy_probe", source)
    managed = work / "managed"
    managed.mkdir()
    for name in ["NetMQ.dll", "AsyncIO.dll"]:
        shutil.copyfile(args.managed_directory / name, managed / name)
    env = {**os.environ, "DOTNET_CLI_TELEMETRY_OPTOUT": "1", "DOTNET_SKIP_FIRST_TIME_EXPERIENCE": "1",
           "DOTNET_CLI_HOME": str(work / "dotnet-home"), "DOTNET_GENERATE_ASPNET_CERTIFICATE": "false"}
    sdk = subprocess.check_output([args.dotnet, "--version"], text=True, env=env).strip()
    framework = "net" + sdk.split(".")[0] + ".0"
    subprocess.run([args.dotnet, "build", str(source / "NetMQProbe.csproj"), "--nologo",
                    "--ignore-failed-sources", "--output", str(work / "bin"),
                    "-p:TargetFramework=" + framework,
                    "-p:LegacyManagedDirectory=" + str(managed)], check=True, env=env)
    report = dict(netmq_sha256=actual, publisher_sha256=digest(args.publisher), dotnet_sdk=sdk,
                  scope="Exact historical DLL transport/data/reconnect oracle; not unchanged VR scene-reset acceptance",
                  cases=[run_case(args, work, work / "bin/NetMQProbe.dll", mode, env)
                         for mode in ["single", "topic_json"]])
    result = json.dumps(report, indent=2) + "\n"
    (work / "report.json").write_text(result)
    if args.report:
        args.report.write_text(result)
    print(result)


if __name__ == "__main__":
    main()
