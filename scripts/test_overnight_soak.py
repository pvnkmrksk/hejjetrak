"""Safety/measurement tests for the development overnight supervisor."""
import csv
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import tempfile
import sys
import unittest

from overnight_soak import (atomic_json, await_completion, clock_warnings, completion_marker, pid_identity, read_status,
                            record_application_exit, require_generated_directory, storage_projection,
                            trajectory_statistics)


class OvernightEvidenceTests(unittest.TestCase):
    def test_abnormal_native_exit_is_a_terminal_failure(self):
        state = {"state": "completed"}
        record_application_exit(state, 0)
        self.assertEqual(state["state"], "completed")
        record_application_exit(state, -15)
        self.assertEqual(state["state"], "failed")
        self.assertEqual(state["application_exit_code"], -15)
        self.assertIn("-15", state["error"])
        state = {"state": "failed", "error": "earlier acquisition failure"}
        record_application_exit(state, 1)
        self.assertEqual(state["error"], "earlier acquisition failure")
        self.assertIn("status 1", state["shutdown_error"])

    def test_velocity_excludes_missing_motion_and_uses_recorded_time(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            states = ["initializing", "tracked", "rejected", "relocalized", "tracked"]
            accepted = [True, True, False, True, True]
            rows = []
            for frame in range(5):
                row = [0.0] * 25
                row[0], row[21], row[23] = frame, frame * 10, 10 if frame else 0
                row[1] = .01 if frame == 1 else .02 if frame == 4 else 0
                row[18] = .02 if frame == 1 else .03 if frame == 4 else 0
                rows.append(row)
            with (directory / "epoch-0000-all-frames.dat").open("w", newline="") as stream:
                csv.writer(stream).writerows(rows)
            with (directory / "epoch-0000-quality.csv").open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=["frame", "timestamp_ms", "accepted", "status", "fit_error"])
                writer.writeheader()
                for frame, (state, valid) in enumerate(zip(states, accepted)):
                    writer.writerow({"frame": frame, "timestamp_ms": frame * 10,
                                     "accepted": str(valid).lower(), "status": state, "fit_error": 100})
            result = trajectory_statistics(directory)
            self.assertEqual(result["motion_measurements"], 2)
            self.assertEqual(result["missing_motion_rows"], 3)
            self.assertEqual(result["angular_speed_rad_per_second_p95"], 2)
            self.assertEqual(result["translation_sphere_rad_per_second_p95"], 3)
            self.assertEqual(result["longest_unaccepted_run"], 1)
            self.assertEqual(result["source_interval_ms_maximum"], 10)
            rows[-1][21], rows[-1][23] = 150, 120
            with (directory / "epoch-0000-all-frames.dat").open("w", newline="") as stream:
                csv.writer(stream).writerows(rows)
            gapped = trajectory_statistics(directory)
            self.assertEqual(gapped["nominal_missing_interval_estimate"], 11)
            self.assertEqual(gapped["angular_speed_rad_per_second_p95"], 1)
            self.assertIn("120.000 ms", clock_warnings(gapped)[0])
            result["rotation_source_interval_ms"] = 75
            self.assertIn("rotation interval is 75.000 ms", clock_warnings(result)[0])
            result["rotation_source_interval_ms"] = 50
            self.assertEqual(clock_warnings(result), [])

    def test_missing_pending_failed_and_complete_recording_markers(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            take = output / "segments" / "000001" / "session-1"
            take.mkdir(parents=True)
            self.assertIsNone(completion_marker(take))
            with self.assertRaisesRegex(RuntimeError, "missing/pending"):
                await_completion(output, take, True, timeout=0)
            marker = {"directory": str(take), "state": "finalizing", "numeric_error": None,
                      "completion_error": None, "media_warnings": []}
            atomic_json(take / "completion.json", marker)
            self.assertIsNone(completion_marker(take))
            with self.assertRaisesRegex(RuntimeError, "missing/pending"):
                await_completion(output, take, True, timeout=0)
            marker.update(state="failed", numeric_error="injected numeric fsync failure")
            atomic_json(take / "completion.json", marker)
            with self.assertRaisesRegex(RuntimeError, "fsync failure"):
                completion_marker(take)
            marker.update(state="complete", numeric_error=None)
            atomic_json(take / "completion.json", marker)
            self.assertEqual(await_completion(output, take, True, timeout=0)["state"], "complete")
            marker["directory"] = str(output / "different-take")
            atomic_json(take / "completion.json", marker)
            with self.assertRaisesRegex(RuntimeError, "different take"):
                completion_marker(take)

    def test_retention_boundary_rejects_external_tree_and_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            output = base / "run"
            generated = output / "segments" / "000001" / "session-1"
            generated.mkdir(parents=True)
            original = base / "original-camera-video.avi"
            original.write_bytes(b"private original input must survive")
            require_generated_directory(output, generated)
            with self.assertRaises(ValueError):
                require_generated_directory(output, base)
            media = generated / "epoch-0000-media"
            media.mkdir()
            (media / "raw.avi").symlink_to(original)
            with self.assertRaises(ValueError):
                require_generated_directory(output, generated)
            self.assertEqual(original.read_bytes(), b"private original input must survive")

    @unittest.skipUnless(sys.platform.startswith("linux"), "PID start ticks require Linux /proc")
    def test_status_checks_freshness_and_pid_start_ticks(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            identity = pid_identity(os.getpid())
            state = {"state": "running", "runner": identity, "application": identity,
                     "updated_utc": datetime.now(timezone.utc).isoformat(), "warnings": []}
            atomic_json(output / "status.json", state)
            self.assertTrue(read_status(output)["healthy_running"])
            state["runner"] = {"pid": identity["pid"], "start_ticks": identity["start_ticks"] + 1}
            atomic_json(output / "status.json", state)
            self.assertFalse(read_status(output)["healthy_running"])
            state["runner"] = identity
            state["updated_utc"] = (datetime.now(timezone.utc) - timedelta(seconds=100)).isoformat()
            atomic_json(output / "status.json", state)
            self.assertFalse(read_status(output)["healthy_running"])
            state["updated_utc"] = datetime.now(timezone.utc).isoformat()
            state["warnings"] = ["incomplete video"]
            atomic_json(output / "status.json", state)
            result = read_status(output)
            self.assertTrue(result["operational_running"])
            self.assertFalse(result["healthy_running"])

    def test_storage_projection_protects_numerics_before_optional_movie(self):
        value = storage_projection(3 * 1024**3, 400000, 3600, 4, 3)
        self.assertFalse(value["raw_probe_allowed"])
        self.assertFalse(value["source_video_allowed"])
        self.assertFalse(value["numerics_at_risk"])
        self.assertTrue(storage_projection(512 * 1024**2, 400000, 3600, 4, 3)["numerics_at_risk"])


if __name__ == "__main__":
    unittest.main()
