# Video and offline tracking

Open a file through the source selector or drop it onto the native view. GUI playback follows presentation timestamps, keeping the source rate. Offline CLI and headless agents normally process as quickly as the pipeline permits; optional agent `realtime` pacing is available.

## Supported inputs

The tested adapters support H.264 MP4/MOV/AVI with presentation-order B-frame handling; MPEG-4 Part 2 MP4/MOV/AVI; MJPEG and uncompressed AVI; and mono/4:2:0 Y4M. Codec profiles and unusual containers outside tested coverage can fail. Failures identify the unsupported or malformed input rather than invoking a hidden external decoder.

All application decoding is Rust. File decoding and tracking overlap through a one-slot bounded producer. File frames are retained in order rather than dropped to make the displayed rate look healthy. The synchronous decoder remains available to regression oracles.

## Deterministic command-line run

From the extracted distribution folder:

```sh
./hejjetrac-cli track --config examples/benchmark/config.txt --input examples/sample.mp4 --output runs/sample-001
./hejjetrac-cli compare --reference examples/benchmark/upstream.dat --candidate runs/sample-001/tracking.dat --report runs/sample-001/comparison.json
```

On Windows, use `hejjetrac-cli.exe`. On Mac, the command-line tool and resources are under the app's `Contents/MacOS` and `Contents/Resources` directories; use absolute paths, or run this from the chosen app bundle (replace the initial path):

```sh
cd /path/to/hejjetrac.app/Contents/Resources
../MacOS/hejjetrac-cli track --config examples/benchmark/config.txt --input examples/sample.mp4 --output /tmp/hejjetrac-sample-001
```

Use a new output path for each run.

The output directory must be new. The bundled sample has 300 frames. Comparison aligns frame counts and video timestamps, then checks per-axis delta, path and wrapped heading limits. Its reference is another implementation's output, not independently measured physical ground truth.

## Source time and edits

Recorded PTS are preserved. Missing or nonmonotonic timestamps are errors. An explicit positive `src_fps` override changes emitted intervals while preserving source-clock continuity during an edit. Undo/redo retains source position when input is unchanged. `reset` resets tracking state without seeking; `rewind` reopens the source. Native Step/Advance skips playback waiting for the requested frame budget while retaining the original emitted timestamps. Ordinary Play follows source deadlines; changing the FPS override resets its wall-time anchor without jumping the emitted clock.

Inspection is available without tracking:

```sh
./hejjetrac-cli inspect examples/sample.mp4 --decode
./hejjetrac-cli inspect examples/sample.mp4 --preview first.pgm
```

Read [output semantics](output.md) before deriving velocities from timestamps, especially across rejected intervals.
