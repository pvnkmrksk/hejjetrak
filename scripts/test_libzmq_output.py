#!/usr/bin/env python3
"""Independent libzmq subscriber oracle for native FT and both MATREX framings.

Test tooling only: requires PyZMQ; the production publisher is the Rust CLI.
Every observed frame is checked against that run's accepted real-video rows.
PUB startup loss is permitted; payload changes, nonfinite values and reordering
are not. Run the unchanged-player suite separately for legacy VR acceptance.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import socket
import subprocess
import tempfile
import time

import zmq

ROOT = Path(__file__).resolve().parents[1]


def configure(text, fields):
    for key, value in fields.items():
        pattern = r"(?m)^" + re.escape(key) + r"\s*:.*$"
        line = key + " : " + value
        if re.search(pattern, text):
            text = re.sub(pattern, lambda _: line, text)
        else:
            text += "\n" + line + "\n"
    return text


def expected_json(line):
    fields = ["FT", *line.decode("utf-8").split(",")]
    return json.dumps(dict(x=float(fields[15]), y=float(fields[16]), z=0.0,
                           yaw=math.degrees(float(fields[17])), pitch=0.0, roll=0.0)).encode()


def run_case(cli, work, mode, framing, bind_host, connect_host):
    case = work / (mode + "-" + framing)
    case.mkdir()
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    config = configure((ROOT / "examples/benchmark/config.txt").read_text(),
                       {"sock_output": "n", "sock_port": "-1", "zmq_port": str(port),
                        **({} if bind_host is None else {"zmq_host": bind_host}),
                        **({} if mode == "native_ft" else {
                            "zmq_output_mode": mode, "zmq_framing": framing, "zmq_topic": "FT"})})
    (case / "config.txt").write_text(config)
    packets = []
    with zmq.Context() as context:
        with context.socket(zmq.SUB) as sub:
            sub.setsockopt(zmq.LINGER, 0)
            sub.setsockopt(zmq.RECONNECT_IVL, 10)
            sub.subscribe(b"FT" if mode == "native_ft" or framing == "topic_json" else b"{")
            sub.connect(f"tcp://{connect_host}:{port}")
            process = subprocess.Popen([str(cli), "track", "--config", str(case / "config.txt"),
                                        "--input", str(ROOT / "examples/sample.mp4"),
                                        "--output", str(case / "output")],
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            deadline = time.monotonic() + 60
            while process.poll() is None:
                if sub.poll(10):
                    packets.append(sub.recv_multipart())
                if time.monotonic() > deadline:
                    process.kill()
                    raise RuntimeError("Rust sample publisher exceeded 60 seconds")
            while sub.poll(100):
                packets.append(sub.recv_multipart())
            stdout, stderr = process.communicate(timeout=5)
    (case / "stdout.txt").write_bytes(stdout)
    (case / "stderr.txt").write_bytes(stderr)
    if process.returncode:
        raise RuntimeError("Rust publisher failed: " + stderr.decode(errors="replace"))
    rows = (case / "output/tracking.dat").read_bytes().splitlines(keepends=True)
    expected = {(b"FT, " + row if mode == "native_ft" else expected_json(row)): index
                for index, row in enumerate(rows)}
    observed = []
    for frames in packets:
        if mode == "native_ft" or framing == "single":
            if len(frames) != 1:
                raise RuntimeError("Single output contains extra frames")
        elif len(frames) != 2 or frames[0] != b"FT":
            raise RuntimeError("Topic output differs from [FT, JSON]")
        if frames[-1] not in expected:
            raise RuntimeError("Wire bytes differ from accepted numeric row: " + repr(frames))
        observed.append(expected[frames[-1]])
    if not observed or any(a >= b for a, b in zip(observed, observed[1:])):
        raise RuntimeError("Subscriber received no rows, duplicates or reordered data")
    summary = json.loads((case / "output/summary.json").read_text())
    return dict(mode=mode, framing=framing, bind_host=bind_host,
                connect_host=connect_host, source_frames=summary["frames"],
                recorded_rows=len(rows), received_rows=len(observed),
                first_row_index=observed[0], last_row_index=observed[-1],
                all_received_bytes_exact=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--workdir", type=Path)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--implicit-bind", action="store_true",
                        help="Omit zmq_host to exercise the shared wildcard default")
    parser.add_argument("--connect-host", default="127.0.0.1",
                        help="Subscriber's numeric address for the publishing computer")
    args = parser.parse_args()
    work = args.workdir or Path(tempfile.mkdtemp(prefix="hejjetrac-libzmq-"))
    if args.workdir:
        work.mkdir(parents=True, exist_ok=False)
    cli = args.cli.resolve()
    report = dict(pyzmq=zmq.__version__, libzmq=zmq.zmq_version(),
                  cli_sha256=hashlib.sha256(cli.read_bytes()).hexdigest(),
                  input_sha256=hashlib.sha256((ROOT / "examples/sample.mp4").read_bytes()).hexdigest(),
                  scope="Modern libzmq wire oracle over all 300 real sample frames; no legacy VR acceptance claim",
                  cases=[run_case(cli, work, mode, framing,
                                  None if args.implicit_bind else "127.0.0.1",
                                  args.connect_host) for mode, framing in [
                      ("native_ft", "single"), ("legacy_matrex_vr", "single"),
                      ("legacy_matrex_vr", "topic_json")]])
    result = json.dumps(report, indent=2) + "\n"
    (work / "report.json").write_text(result)
    if args.report:
        args.report.write_text(result)
    print(result)


if __name__ == "__main__":
    main()
