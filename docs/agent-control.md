# Local agent and command-line control

Native `status` separates `tracking_ms` (arithmetic mean of the shared numerical
engine) and `tracking_samples` from `processing_ms` (smoothed worker cost).
Tracking time excludes capture, conversion, engine construction, recording and
preview publication; it includes ROI preparation and numerical maps. The mean
restarts after configuration or tracking reset. `processing_fps` remains the
instantaneous call rate for existing clients, including initial engine setup;
neither value is source FPS. See [tracking-stage measurements](tracking-performance.md).

`fictrac` is a complete headless frontend to the shared Rust engine. External AI agents can operate it through JSONL or MCP stdio; neither requires a model API key, a network service, Python or a GUI. The desktop also has its own asynchronous stdio bridge; its playback scheduling differs from deterministic recorded-video commands. Numerical fitting and calibration use the same core functions.

The design follows [PhotoCraft's engine-first architecture](https://github.com/storytold/photocraft/blob/main/docs/architecture.md): native user interfaces and machine interfaces call reusable operations. This is an agent control surface, not a claim that a language model has measured scientific accuracy. The full supplied video is processed by the transcript regression and checked against the existing reference gates.

```sh
cargo run -p fictrac --release --locked -- --help
cargo run -p fictrac --release --locked -- schema
cargo run -p fictrac --release --locked -- agent < examples/agents/sample-session.jsonl
```

`schema` and the `discover` command return descriptions and a JSON Schema 2020-12 for every request. Protocol version is `1`. Each input line is one JSON object, with `id`, `command`, optional `session`, and `args`. Each output line is one response with the same ID, `ok`, `result` and `error`. Diagnostics never mix with protocol stdout. Syntax/argument errors return structured responses and leave the server available for the next command.

```json
{"id":1,"command":"open","args":{"config":"examples/benchmark/config.txt","input":"examples/sample.mp4"}}
{"id":2,"command":"step","session":1,"args":{"frames":3}}
{"id":3,"command":"config","session":1}
{"id":4,"command":"set","session":1,"args":{"fields":{"thr_ratio":"1.25","opt_max_evals":"50"}}}
{"id":5,"command":"run","session":1,"args":{"frames":300}}
{"id":6,"command":"export","session":1,"args":{"directory":"runs/agent-export"}}
```

The controller processes commands synchronously. `start` arms a session; explicit `step`/`run` advance it. There is no hidden background loop. `realtime` with `args.enabled=true` optionally paces explicit headless `step`/`run` to emitted video timestamps; it defaults to false for bulk processing. Use short frame batches when interactive responsiveness is needed. Native ordinary playback uses the same setting on its asynchronous worker. Explicit native Step/Advance processes the requested frame budget without waiting for playback deadlines; emitted source timestamps are not retimed. An FPS edit resets only the pacing wall anchor while preserving emitted-clock continuity. A run processes at most 10,000 frames, reports the processed count and end-of-stream status, and can be continued with another run. Up to 16 sessions are independent. Closing one native window/session, including the initial window, does not close the remaining sessions; explicit application shutdown terminates them all. `new_instance` chooses the next existing BR/VR or numbered config, otherwise copies the document; numeric camera IDs increment while video paths remain paths. Recorded-video control explicitly rejects live camera sources; live capture remains available through the native desktop.

Configuration is a lossless legacy document. `config` returns raw text, fields and effective numerical settings. `set` accepts one key/value or a batch of legacy value strings; `replace_config` accepts complete text. Validation happens before committing an edit. A failed edit preserves state. Successful numerical edits keep the current source position, initialize a new tracking epoch on the current image, and preserve the next video frame. A source change or `rewind` reopens the video. Changes to `src_fps` preserve the current emitted clock and adjust subsequent intervals; disabling an override resumes source intervals with a continuity offset. Unchanged default playback retains original timestamps exactly. `undo`/`redo` use the same validation and epoch semantics. `save_config` writes an explicitly named new file. Unknown fields and comments are retained as data.

`calibrate` provides graphical calibration operations in source pixels: `rim`, `exclude`, `replace_exclusions`, `remove_exclusion`, `forward`, `square` and `square_cycle`. `replace_exclusions` takes `polygons` as a complete array of source-pixel polygons; an empty array explicitly clears all masks in one validated transaction. Rim points fit the saved sphere centre/radius and retain the clicked outline. Square corners retain upstream's plane-specific correspondence: XY `(+X,-Y), (+X,+Y), (-X,+Y), (-X,-Y)`; YZ `(-Y,-Z), (+Y,-Z), (+Y,+Z), (-Y,+Z)`; XZ `(+X,-Z), (-X,-Z), (-X,+Z), (+X,+Z)`. These are TL, TR, BR, BL when looking from above, behind, or the animal's left side respectively; the opposite view reverses the indicated handedness. Forward uses `start` and `end` points. These call the same core calibration implementation as the native GUI. `square_cycle` performs the F reflection/re-fit operation using saved `c2a_src`, corners and pose; it never rotates or relabels the corner order. The bounded Rust fit preserves proper rotations and reports unit-ray reprojection error and orientation change. Some views have no second accurate pose; that returns an error and preserves the previous configuration. Its deterministic fit differs from upstream's NLopt global optimizer.

`calibrate` additionally exposes `resolution_reference` and `resolution_apply`. Reference capture accepts optional `width`/`height` (default current source) and stores `rust_calibration_size`, projection/FOV and normalized extension copies of rim, exclusions and square marks while retaining ordinary pixel fields. Apply requires explicit target dimensions and regenerates only pixel marks for identical aspect ratio and configured field of view/projection; angular `roi_c`/`roi_r` and `c2a_r`/`c2a_t` remain unchanged. Stale marks or incompatible framing reject before mutation. A metadata-free legacy config is never normalized implicitly.

`reset` matches the tracker reset: source position, frame count and integrated displacement continue, while relative sequence, pose, path and heading reset. `rewind` explicitly starts the source again. `map_save` and `map_load` export/import real equal-area sphere templates. Import validates dimensions before mutation.

`rows` returns bounded numerical rows including acceptance/status. `snapshot` writes source luma, source color, ROI, warped difference, instant/accumulated maps and the calibrated six-panel debug mosaic as portable PGM/PPM files. `export` writes legacy accepted-row `tracking.dat`, full `all-frames.dat`, all-frame `quality.csv`, config and path SVG. Rejected frames remain observable; frozen paths do not establish zero motion.

`record_start` opens a new directory and streams the same data plus optional raw/debug videos controlled by `save_raw`, `save_debug` and `vid_codec`. Optional `args.record_options={"raw":true,"debug":false}` selects raw only; use false/true for debug, true/true for both, or false/false for data only. This transient selection does not edit canonical config, reset the numerical engine or advance/rewind the source. Omit it to use canonical flags. The recording saves an effective config copy and status exposes the actual selection. `record_stop`, source EOF, `close`, or normal stdin EOF finalizes encoders. A configuration edit closes recording rather than mixing calibrations. Start recording again for the new epoch. Legacy recording codecs are implemented in Rust: `h264` (AVI), `xvid` (MPEG-4 Visual AVI), `mpg4` (MPEG-4 Visual MP4), `mjpg` (MJPEG AVI), and `raw` (uncompressed AVI). The additional `h264_mp4` choice selects MP4. Even source dimensions are preserved by H.264 macroblock edge padding plus SPS cropping; odd 4:2:0 recording dimensions fail explicitly. Source decoding includes those supported AVI/MP4 codecs and mono/420 Y4M. `discover` advertises formats; detailed container limits remain in the output documentation. Configured serial/UDP sinks receive accepted rows; sink write failures are reported in status warnings. No shell expressions or document instructions are executed.

Output paths must be new. Requests are bounded to 1 MB, rows to one million per session and queries to 10,000 rows. Exhausting a bound returns an actionable error. The process has its ordinary local filesystem permissions; applications connecting an AI client should grant only the local access they intend that client to use.

## MCP stdio

Register the release executable as a local MCP server:

```json
{"mcpServers":{"fictrac":{"command":"/absolute/path/to/hejjetrac-cli","args":["mcp"]}}}
```

Use the packaged CLI executable's absolute path (`hejjetrac-cli.exe` on Windows; `hejjetrac.app/Contents/MacOS/hejjetrac-cli` on Mac). Cargo source builds retain the `fictrac` compatibility name. The adapter implements the [MCP initialization lifecycle](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle), `ping`, `tools/list` and `tools/call` for protocol `2025-06-18`. Its `fictrac_execute` tool advertises the concrete request schema and calls the same controller as JSONL. Results include both text JSON and `structuredContent`; command failures set `isError`. Notifications receive no response. Closing stdin shuts down cleanly. The shared `close` arguments include optional `discard=true`; this explicitly permits discarding unsaved native GUI edits. Headless sessions are disposable in-memory state and close directly. No HTTP listener, cloud connection or credential is created.

## Conventional CLI compatibility

```sh
fictrac track --config config.txt --input recording.mp4 --output runs/experiment
fictrac config.txt --max-frames 300 --output runs/experiment
fictrac config --config config.txt --set thr_ratio=1.25 --set opt_do_global=y --output revised.txt
fictrac compare --reference reference.dat --candidate runs/experiment/tracking.dat
```

The legacy positional config invocation maps to `track`; with no output flag it chooses a new timestamped directory using `output_fn` as its prefix (including any parent path), otherwise the input video stem, otherwise `fictrac`. Unlike upstream flat timestamped filenames, related artifacts are grouped inside this directory. `config` prints an edited document when `--output` is omitted. Arbitrary calibration editing and multi-session orchestration are exposed through the agent protocol rather than requiring GUI automation. CLI inspection, synthetic demo, rim calibration and benchmark comparison remain available; see each command's `--help`.

## Native display and camera controls

The native desktop JSONL bridge advertises `view` with optional boolean `settings`, `diagnostics`, `overlays`, `full_image`, `edit_marks` and `magnifier` fields, plus `calibration_step` as `view`, `ball`, `exclude` or `lab`. The selected step starts read-only saved review and limits visible geometry.
`calibration_action` accepts `inspect`, `edit`, `redraw`, `add` or `keep`:
inspect cancels an uncommitted draft back to saved review; edit enables saved
marks; redraw starts an isolated replacement; add appends an exclusion; keep
preserves/advances the current review step. `edit_marks` remains a compatibility
control for saved-point editing, and `magnifier` toggles the precision loupe.
Returned view status includes the review action. Display-only flags do not alter
numerical geometry; calibration actions can pause/resume or commit a validated
pending replacement. Poll revision/status to observe completion. Invalid fields fail before any display state changes. Native status also exposes processing milliseconds, actual/source rate, headroom, effective recording selection, camera readback and output queue counters.

`camera_status` requests live capability/readback data. `camera_set` accepts `{control:"exposure_mode",mode:"automatic"|"locked"}` or `{control:"exposure"|"gain",value:number}`. Numeric units and limits come from the actual device; there is no universal millisecond or gain scale. Unsupported controls return an actionable reason. `camera_format` takes exact `width`, `height` and optional `fps` (null means automatic), and intentionally reopens the stream through the same native session path as the GUI. Its persistent format uses `rust_camera_width`, `rust_camera_height` and `src_fps`; unknown legacy fields remain preserved.

Recorded-video headless discovery omits these native-only commands. Explicit calls return a native-session error rather than pretending to control a camera or display. Video sessions in the desktop similarly reject camera controls. Output status exposes UDP/serial enablement plus ZeroMQ endpoint, queue capacity/length, enqueued/published/dropped/failure counters and publisher health. Unchanged output configuration retains the publisher during calibration edits, undo and rewind; applying settings can restart a failed publisher.

Native status also exposes bounded-worker resource counters: `resources.preview_updates`, `resources.idle_waits` and `resources.pacing_waits`. Preview publication is coalesced separately from tracking and recorded media. Paused/EOF waits and source deadlines are command-interruptible; these counters are scheduling evidence, not measured CPU percentage or power use.
