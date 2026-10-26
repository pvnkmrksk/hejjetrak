"""Development-only real libzmq load for native multi-rig validation."""
from collections import Counter, deque
import csv
import math
import socket
import threading
import time


class NativeWireProbe:
    def __init__(self, ports):
        import zmq
        self.zmq = zmq
        self.ports = list(ports)
        self.packets = {port: deque(maxlen=10000) for port in ports}
        self.received = {port: 0 for port in ports}
        self.incomplete_connections = {port: 0 for port in ports}
        self.errors = []
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self._receive, daemon=True)
        self.thread.start()

    def _receive(self):
        zmq = self.zmq
        context = zmq.Context()
        healthy, slow, incomplete = {}, [], []
        try:
            poller = zmq.Poller()
            for port in self.ports:
                sub = context.socket(zmq.SUB)
                sub.setsockopt(zmq.LINGER, 0)
                sub.setsockopt(zmq.RCVHWM, 10000)
                sub.setsockopt(zmq.RECONNECT_IVL, 10)
                sub.subscribe(b"FT, ")
                sub.connect(f"tcp://127.0.0.1:{port}")
                healthy[sub] = port
                poller.register(sub, zmq.POLLIN)
                # Complete subscription, but never drain application messages.
                stalled = context.socket(zmq.SUB)
                stalled.setsockopt(zmq.LINGER, 0)
                stalled.setsockopt(zmq.RCVHWM, 1)
                stalled.subscribe(b"FT, ")
                stalled.connect(f"tcp://127.0.0.1:{port}")
                slow.append(stalled)
            next_incomplete = 0.0
            while not self.stop.is_set():
                if time.monotonic() >= next_incomplete:
                    for peer in incomplete:
                        peer.close()
                    incomplete.clear()
                    for port in self.ports:
                        try:
                            peer = socket.create_connection(("127.0.0.1", port), timeout=0.03)
                            # Deliberately withhold the ZMTP greeting.
                            incomplete.append(peer)
                            self.incomplete_connections[port] += 1
                        except OSError:
                            pass  # The publisher may be reopening during source changes.
                    next_incomplete = time.monotonic() + 2.0
                for sub, events in poller.poll(10):
                    if events & zmq.POLLIN:
                        while True:
                            try:
                                parts = sub.recv_multipart(zmq.NOBLOCK)
                            except zmq.Again:
                                break
                            if len(parts) != 1 or not parts[0].startswith(b"FT, "):
                                raise RuntimeError("native FT publisher changed its single-frame prefix")
                            payload = parts[0]
                            numbers = [float(v) for v in payload[4:].split(b",")]
                            if len(numbers) != 25 or not all(math.isfinite(v) for v in numbers):
                                raise RuntimeError("native FT packet lacks 25 finite numeric fields")
                            port = healthy[sub]
                            self.received[port] += 1
                            self.packets[port].append(payload)
        except Exception as error:
            self.errors.append(str(error))
        finally:
            for peer in incomplete:
                peer.close()
            for sub in [*healthy, *slow]:
                sub.close(linger=0)
            context.term()

    def finish(self):
        self.stop.set()
        self.thread.join(timeout=5)
        if self.thread.is_alive():
            raise RuntimeError("libzmq validation subscriber did not stop")
        if self.errors:
            raise RuntimeError("libzmq validation failed: " + "; ".join(self.errors))
        return {
            "pyzmq": self.zmq.__version__, "libzmq": self.zmq.zmq_version(),
            "healthy_subscribers": len(self.ports), "undrained_subscribers": len(self.ports),
            "incomplete_greeting_reconnect_interval_seconds": 2,
            "received_packets": self.received,
            "incomplete_tcp_connections": self.incomplete_connections,
        }

    def compare_recordings(self, directories, minimum_fraction=0.95):
        cases = []
        for port, directory in zip(self.ports, directories):
            rows = (directory / "epoch-0000.dat").read_bytes().splitlines(keepends=True)
            all_rows = (directory / "epoch-0000-all-frames.dat").read_bytes().splitlines()
            with (directory / "epoch-0000-quality.csv").open(newline="") as stream:
                quality = list(csv.DictReader(stream))
            if len(all_rows) != len(quality) or sum(row["accepted"] == "true" for row in quality) != len(rows):
                raise RuntimeError(f"port {port}: numeric/quality evidence is incomplete")
            expected = {b"FT, " + row: index for index, row in enumerate(rows)}
            observed = [expected[packet] for packet in self.packets[port] if packet in expected]
            ordered = all(a < b for a, b in zip(observed, observed[1:]))
            fraction = len(observed) / max(1, len(rows))
            if not rows or not ordered or fraction < minimum_fraction:
                raise RuntimeError(f"port {port}: recorded/wire agreement failed: "
                                   f"{len(observed)}/{len(rows)}, ordered={ordered}")
            if self.incomplete_connections[port] == 0:
                raise RuntimeError(f"port {port}: incomplete peer load was not exercised")
            cases.append({"port": port, "recorded_all_frame_rows": len(all_rows),
                          "quality_statuses": dict(Counter(row["status"] for row in quality)),
                          "accepted_fraction": len(rows) / max(1, len(all_rows)),
                          "recorded_accepted_rows": len(rows),
                          "received_exact_recorded_rows": len(observed),
                          "received_fraction": fraction, "all_matched_bytes_exact": True,
                          "ordered_without_duplicates": ordered,
                          "minimum_received_fraction": minimum_fraction})
        return cases
