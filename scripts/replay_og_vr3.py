"""Run an existing upstream FicTrac binary on the private VR3 AVI as an oracle.

Development comparison only: the Rust application never imports this script or
uses the upstream binary. Original inputs/configs are read-only. All generated
files, including upstream's inferred output prefix, stay in a NEW directory.
"""
import argparse
import json
from pathlib import Path
import subprocess
import time

from benchmark_vr3 import POST, STEM, dat_rows, set_field, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--reference-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    source = args.reference_dir.resolve(strict=True)
    raw = source / f"fictrac-raw-{STEM}.avi"
    config = (source / "config.txt").read_text()
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    # Upstream otherwise derives an output prefix from src_fn, which would put
    # output alongside the original recording. The local symlink and explicit
    # output_fn each keep its artifacts inside this dedicated analysis directory.
    (output / "input.avi").symlink_to(raw)
    for key, value in [("src_fn", "input.avi"), ("output_fn", "og-replay"),
                       ("do_display", "n"), ("save_raw", "n"), ("save_debug", "n"),
                       ("sock_port", "-1"), ("com_port", "")]:
        config = set_field(config, key, value)
    copied = output / "config.txt"
    copied.write_text(config)
    manifest = {"binary": str(binary), "binary_sha256": sha256(binary),
                "input": str(raw), "input_sha256": sha256(raw),
                "copied_config_sha256_before_run": sha256(copied),
                "command": [str(binary), "config.txt"], "cwd": str(output),
                "scope": "Upstream executable used only as a development oracle; no application dependency."}
    started = time.perf_counter()
    with (output / "console.log").open("w") as log:
        completed = subprocess.run(manifest["command"], cwd=output,
                                   stdout=log, stderr=subprocess.STDOUT)
    manifest.update({"returncode": completed.returncode,
                     "wall_seconds": time.perf_counter() - started,
                     "copied_config_sha256_after_run": sha256(copied)})
    data_files = list(output.glob("*.dat"))
    if completed.returncode == 0 and len(data_files) == 1:
        rows = dat_rows(data_files[0])
        manifest.update({"dat": str(data_files[0]), "dat_sha256": sha256(data_files[0]),
                         "frames": len(rows),
                         "aligned": len(rows) == POST[1] and all(row[0] == i for i, row in enumerate(rows))})
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    if completed.returncode != 0 or not manifest.get("aligned"):
        raise RuntimeError("upstream replay did not produce all 5,157 aligned rows; inspect console.log and manifest.json")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
