#!/usr/bin/env python3
"""Check the bridge launcher's isolated configs and owned-process cleanup."""
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time
import unittest

SOURCE_ROOT = Path(__file__).resolve().parent.parent

CLI_STUB = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import re
import sys
args = sys.argv[1:]
assert args[0] == "config"
source = Path(args[args.index("--config") + 1])
output = Path(args[args.index("--output") + 1])
assert args[args.index("--set") + 1] == "zmq_port=0"
with open(os.environ["LAUNCH_LOG"], "a") as log:
    log.write(json.dumps({"source": str(source), "output": str(output)}) + "\n")
if os.environ.get("FAIL_RIG") == source.stem:
    sys.exit(19)
text, count = re.subn(r"(?m)^(zmq_port\s*:\s*)\d+", r"\g<1>0", source.read_text())
assert count == 1
with output.open("x") as stream:
    stream.write(text)
'''

APP_STUB = r'''#!/usr/bin/env python3
import json
import os
from pathlib import Path
import signal
import sys
import time
args = sys.argv[1:]
assert args[args.index("--windows") + 1] == "4"
assert args[args.index("--grid") + 1] == "2x2"
folder = Path(args[args.index("--config") + 1]).parent
configs = {f"VR{i}.txt": (folder / f"VR{i}.txt").read_text() for i in range(1, 5)}
def terminated(received, frame):
    assert all((folder / f"VR{i}.txt").exists() for i in range(1, 5))
    Path(os.environ["SIGNAL_LOG"]).write_text(str(received))
    sys.exit(0)
signal.signal(signal.SIGTERM, terminated)
ready = Path(os.environ["APP_READY"])
pending = ready.with_suffix(".partial")
pending.write_text(json.dumps({"folder": str(folder), "configs": configs}))
pending.replace(ready)
if os.environ.get("WAIT_FOR_SIGNAL"):
    while True:
        time.sleep(0.05)
sys.exit(int(os.environ.get("APP_EXIT", "0")))
'''


@unittest.skipUnless(os.name == "posix" and shutil.which("sh"), "requires a POSIX shell")
class BridgedLauncherTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="hejjetrac-launcher-test-")
        self.root = Path(self.temporary.name)
        self.repo = self.root / "repo with spaces"
        (self.repo / "scripts").mkdir(parents=True)
        self.launcher = self.repo / "scripts/run_four_bridged.sh"
        shutil.copyfile(SOURCE_ROOT / "scripts/run_four_bridged.sh", self.launcher)
        self.originals = {}
        for index in range(1, 5):
            name = f"VR{index}.txt"
            self.originals[name] = (SOURCE_ROOT / name).read_bytes()
            (self.repo / name).write_bytes(self.originals[name])
        self.bin_dir = self.root / "paired app"
        self.bin_dir.mkdir()
        self.app = self.executable(self.bin_dir / "hejjetrac", APP_STUB)
        self.cli = self.executable(self.bin_dir / "hejjetrac-cli", CLI_STUB)
        self.scratch = self.root / "scratch"
        self.scratch.mkdir()
        self.log = self.root / "cli.jsonl"
        self.ready = self.root / "ready.json"
        self.signal_log = self.root / "signal.txt"
        self.env = dict(os.environ, HEJJETRAC_BINARY=str(self.app),
                        TMPDIR=str(self.scratch), LAUNCH_LOG=str(self.log),
                        APP_READY=str(self.ready), SIGNAL_LOG=str(self.signal_log))
        self.env.pop("HEJJETRAC_CLI", None)
        self.env.pop("WAIT_FOR_SIGNAL", None)
        self.env.pop("FAIL_RIG", None)
        self.env.pop("APP_EXIT", None)

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def executable(path, text):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        path.chmod(0o755)
        return path

    def launch(self):
        return subprocess.run(["sh", str(self.launcher)], env=self.env,
                              text=True, capture_output=True, timeout=10)

    def assert_originals_and_cleanup(self):
        for name, original in self.originals.items():
            self.assertEqual((self.repo / name).read_bytes(), original)
        self.assertEqual(list(self.scratch.glob("hejjetrac-bridged.*")), [])

    def assert_four_overrides(self):
        state = json.loads(self.ready.read_text())
        calls = [json.loads(line) for line in self.log.read_text().splitlines()]
        self.assertEqual(len(calls), 4)
        folder = Path(state["folder"])
        for index, call in enumerate(calls, 1):
            name = f"VR{index}.txt"
            self.assertEqual(Path(call["source"]), self.repo / name)
            self.assertEqual(Path(call["output"]), folder / name)
            expected, count = re.subn(r"(?m)^(zmq_port\s*:\s*)\d+", r"\g<1>0",
                                      self.originals[name].decode())
            self.assertEqual(count, 1)
            self.assertEqual(state["configs"][name], expected)
        self.assertFalse(folder.exists())
        self.assert_originals_and_cleanup()

    def test_paired_cli_disables_all_four_publishers_without_editing_rigs(self):
        result = self.launch()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assert_four_overrides()

    def test_explicit_cli_override_and_local_release_cli_fallback(self):
        self.cli.unlink()
        alternate = self.executable(self.root / "alternate cli", CLI_STUB)
        self.env["HEJJETRAC_CLI"] = str(alternate)
        self.assertEqual(self.launch().returncode, 0)
        self.assert_four_overrides()
        self.log.unlink()
        self.ready.unlink()
        self.env.pop("HEJJETRAC_CLI")
        self.executable(self.repo / "target/release/fictrac", CLI_STUB)
        self.assertEqual(self.launch().returncode, 0)
        self.assert_four_overrides()

    def test_failed_config_edit_does_not_start_app_and_cleans_up(self):
        self.env["FAIL_RIG"] = "VR3"
        self.assertEqual(self.launch().returncode, 19)
        self.assertFalse(self.ready.exists())
        self.assert_originals_and_cleanup()

    def test_app_exit_status_is_retained(self):
        self.env["APP_EXIT"] = "23"
        self.assertEqual(self.launch().returncode, 23)
        self.assert_four_overrides()

    def check_signal_cleanup(self, received, exit_code):
        self.env["WAIT_FOR_SIGNAL"] = "1"
        process = subprocess.Popen(["sh", str(self.launcher)], env=self.env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        try:
            deadline = time.monotonic() + 5
            while not self.ready.exists() and process.poll() is None and time.monotonic() < deadline:
                time.sleep(0.02)
            self.assertTrue(self.ready.exists(), "app did not receive all four configs")
            folder = Path(json.loads(self.ready.read_text())["folder"])
            self.assertEqual(len(list(folder.glob("VR*.txt"))), 4)
            process.send_signal(received)
            _, errors = process.communicate(timeout=5)
            self.assertEqual(process.returncode, exit_code, errors)
            self.assertEqual(self.signal_log.read_text(), str(int(signal.SIGTERM)))
            self.assert_four_overrides()
        finally:
            if process.poll() is None:
                process.terminate()
                process.communicate(timeout=5)

    def test_term_reaches_owned_app_before_temporary_configs_are_removed(self):
        self.check_signal_cleanup(signal.SIGTERM, 143)

    def test_interrupt_terminates_owned_background_app_and_cleans_up(self):
        self.check_signal_cleanup(signal.SIGINT, 130)


if __name__ == "__main__":
    unittest.main()
