# hejjetrak-cli(1)

## Name

`hejjetrak-cli` — headless spherical-motion tracking, calibration and local machine control.

Release packages use `hejjetrak-cli`; Cargo's compatibility binary is named `fictrac`. The desktop executable is `hejjetrak` (Cargo: `fictrac-desktop`).

## Synopsis

```text
hejjetrak-cli track --config FILE --input VIDEO --output DIRECTORY [--max-frames N]
hejjetrak-cli CONFIG.txt [--max-frames N] [--output DIRECTORY]
hejjetrak-cli inspect VIDEO [--decode] [--preview FILE.pgm]
hejjetrak-cli compare --reference FILE.dat --candidate FILE.dat [--report FILE.json]
hejjetrak-cli config --config FILE [--set KEY=VALUE ...] [--output NEW_FILE]
hejjetrak-cli calibrate --width N --height N --vfov DEGREES --points JSON --output FILE
hejjetrak-cli demo --output NEW_DIRECTORY [--frames N]
hejjetrak-cli schema
hejjetrak-cli agent
hejjetrak-cli mcp
```

Use `--help` or a command's `--help` for the current generated options. Source builds use `cargo run -p fictrac --release --locked -- COMMAND ...`.

## Description

`track` loads a validated config, decodes source frames in order and runs the shared numerical tracker. It writes accepted legacy data plus all-frame numeric/quality evidence and summary/path files. The output directory must not already exist. `--max-frames` processes a prefix and must be at least two when supplied.

A positional legacy config maps to `track`, selecting its source and, when `--output` is absent, a new timestamped directory derived from `output_fn` or input name. Files are grouped in a directory, rather than upstream's flat timestamped output set.

`inspect --decode` decodes the complete input without fitting; `--preview` writes the first luma image. This checks source compatibility separately from calibration.

`compare` requires matching frame counts and aligned timestamps. Default RMS limits are 0.020 rad per rotation component, 0.075 sphere rad for path and 0.025 rad for wrapped heading. Overrides are available as `--max-rotation-rmse`, `--max-path-rmse` and `--max-heading-rmse`. A failed gate returns nonzero. Initialization is excluded from motion errors; the upstream first-row clock anomaly is handled explicitly.

`calibrate` fits a ball rim from at least three source-pixel points. `--points` is a JSON array of x/y pairs and `--lab-rotation` is an optional Rodrigues vector in radians. It writes a JSON calibration. Full square/exclusion/history operations are exposed through the shared agent schema.

`demo` generates a known-motion sphere video, tracks it and checks rotation accuracy. It is a regression fixture, not a replacement for real-camera calibration.

## Machine interfaces

`schema` prints typed operation discovery and JSON Schema. `agent` serves one JSON request/response per line on stdin/stdout. `mcp` exposes the same controller through local MCP stdio JSON-RPC. Diagnostic logs stay out of protocol stdout. There is no cloud service or model API requirement. Native displayed sessions use `hejjetrak --control-stdio`; queued responses require status polling. See [agent control](agent-control.md).

## Exit status and errors

Successful commands return zero. Invalid arguments, malformed media/config, unsafe dimensions, output conflicts, reference-gate failures and observed I/O failures return nonzero with an actionable error. Existing output files are not overwritten. Interrupted output can leave `.partial` files; read [recording](recording.md) before recovering or interpreting them.

## Examples

```sh
./hejjetrak-cli track --config examples/benchmark/config.txt --input examples/sample.mp4 --output runs/check-001
./hejjetrak-cli compare --reference examples/benchmark/upstream.dat --candidate runs/check-001/tracking.dat
./hejjetrak-cli schema
./hejjetrak-cli agent < examples/agents/sample-session.jsonl
```
