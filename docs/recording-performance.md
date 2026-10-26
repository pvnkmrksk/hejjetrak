# Linux camera recording measurements — 26–27 October 2026

The final release preserves near-100-FPS numeric processing during optional
video overload. Its fastest uncompressed setting completes every requested
raw/debug video on one and four cameras: **20/20 complete videos**, independently
decoded and aligned with all numeric rows. All **22,266 accepted recorded rows**
across the final matrices match actual ZMQ packets byte for byte. Diagnostic
H264/MJPEG still exceed this machine's encoding budget and explicitly retain
partial prefixes; they are not claimed as complete 100-FPS videos. See the
[final report](benchmarks/linux-recording-final-queue8-20261027.json).

Recording originally exposed a real acquisition bottleneck in the synchronous
media path. The data-only desktop held all four physical cameras near 101 FPS,
but diagnostic video substantially reduced the numeric row rate. Those failures
are retained below as the baseline for the recording changes.
The [machine-readable evidence](benchmarks/linux-recording-20261026.json)
preserves throughput, CPU, quality and media alignment separately.

The machine is Ubuntu 22.04 x86-64, Intel i7-6700K (four physical/eight logical
cores), Rust 1.99.1. Each Arducam OV9281 supplies calibrated 320×240 MJPEG at
requested 100 FPS. Raw output is 320×240; diagnostic output is 640×480. Copied
rig configs preserve geometry, exposure and numerical settings; UDP, ZMQ and
serial are disabled to isolate recording. The user’s source configs are not
edited by the benchmark.

The supplied original VR3 live log reports **100.93 average FPS**, with mean
grab/optimizer/map/plot/log/display times of 5.5/4.1/0.2/0.0/0.0/0.1 ms and both
raw and diagnostic recordings present. It supplies the requested throughput
target. It does not include a comparable process CPU measurement, and its
internal stage timing boundaries differ from this native Rust matrix.

Each rig has one chronological numerical worker because its current estimate
and atlas depend on earlier images. The operating system may move that worker
between CPU cores, so per-core activity bars alone do not show whether a design
is better. Compare accuracy, delivered numeric FPS, source-clock gaps, aggregate
CPU and latency together. The original log supplies no comparable CPU figure.

`scripts/recording_matrix.py` runs actual native desktop sessions, one camera
then four, serially. It measures process CPU ticks and processed frame deltas
after warmup. CPU is percent of one core: 400% means four fully occupied cores.
Startup, finalization and later independent media decoding are outside the
sustained interval. Live content changes between cases, so this is not a
physical accuracy comparison. Capture counters do not prove that every sensor
image reached the application.

## Synchronous baseline

The H264 matrix uses eight measured seconds after two warmup seconds. The
explicit MJPEG probe uses four measured seconds after one warmup second.
Both use the same production renderer and numerical engine. No codec default
was changed by the probe.

| Codec | Selection | One camera processed FPS | Four cameras processed FPS | Four-camera CPU |
|---|---|---:|---:|---:|
| H264 | Data only | 100.98 | 100.72–100.97 | 185.3% |
| H264 | Raw | 99.97 | 82.73–89.71 | 439.1% |
| H264 | Debug | 28.14 | 22.16–26.39 | 441.8% |
| H264 | Both | 24.65 | 19.42–21.91 | 441.1% |
| MJPEG | Raw | 101.39 | 100.60–101.60 | 306.5% |
| MJPEG | Debug | 43.23 | 32.30–33.54 | 434.0% |
| MJPEG | Both | 38.52 | 28.08–34.29 | 435.1% |

All completed media files independently decode in full with FFprobe. Their
actual image counts equal the complete numeric and quality row counts; each
accepted-only data file exactly matches accepted all-frame decisions. Alignment
does not compensate for reduced acquisition throughput. The H264 four-camera
debug/both runs also contain rejected quality rows, preserved in the evidence.

## Isolated phase baseline

The development Rust example `profile_recording` reconstructs the saved native
VR3 source and diagnostics. It repeats the unchanged image twenty times,
serially, timing render and pre-rendered codec writes separately. Three poses
and three path points keep overlay history small. PNG decoding, initial render,
recorder construction and finalization are excluded from the per-image means.
Encoding includes RGB conversion, packet draining and AVI writes. This is a
phase profile, not a camera throughput claim.

| Phase | Mean ms/image |
|---|---:|
| Full calibrated diagnostic render, 640×480 | 9.649 |
| H264 raw encode/write, 320×240 | 4.406 |
| H264 debug encode/write, 640×480 | 19.912 |
| MJPEG raw encode/write, 320×240 | 1.809 |
| MJPEG debug encode/write, 640×480 | 8.218 |
| Uncompressed raw write, 320×240 | 0.276 |
| Uncompressed debug write, 640×480 | 1.111 |

The measured renderer repeats substantial static geometric work. Caching that
work can recover CPU time, but the measured H264 diagnostic encoder alone
exceeds a 10 ms/image budget on this fixture. Moving encoding to a worker
cannot create unlimited encoding capacity. Any bounded recording worker must
report overload explicitly while preserving acquisition and complete numeric
evidence; sustained full-video success must be measured separately.

## Cached geometric remap

The compositor now prepares the immutable source-coordinate remap once per
recording epoch. Dynamic RGB samples, numerical views, paths and poses still
render on every image. The independent direct-projection renderer remains the
pixel oracle.

A quiet repeat with the updated release example measures **4.500 ms/image** for
the cached renderer against **10.335 ms/image** for direct projection in the
same process, a 2.30× render-phase speedup. Cache construction takes 6.630 ms
once, outside repeated-frame timing. Cached/direct pixels and the saved
pre-change rendered PNG are byte-identical. The encoder timings remain
19.776 ms/image for H264 diagnostic output and 8.530 ms/image for MJPEG.
This confirms the rendering improvement and the remaining encoder limit;
it does not substitute for the native camera matrix.

The final exact compositor additionally caches bilinear sampling and integer
panel indices and skips the input quadrant that calibration replaces. A serial
20-image phase repeat measures **2.5369 ms/image cached** versus **9.4366 ms/image
direct**, a 3.72× render-phase speedup; one-time setup takes 9.1017 ms. The old,
cached and final rendered PNG bytes remain identical, SHA256
`07df5b068143968559fcc9cffeb58d9b06d20b65fa52d97eb3cfbf762ed7fbf8`.

The post-cache native MJPEG matrix repeats the same one/four-camera selections,
four measured seconds and one warmup second. All numeric, quality and complete
independently decoded media remain aligned.

| MJPEG selection | One camera processed FPS | Four cameras processed FPS | Four-camera CPU |
|---|---:|---:|---:|
| Raw | 100.15 | 100.14–101.39 | 304.4% |
| Debug | 59.64 | 40.50–47.46 | 435.4% |
| Both | 51.19 | 35.24–42.93 | 436.8% |

Raw-only meets the declared 95 processed FPS target on every camera. Diagnostic
and combined recording improve materially but still fail that target. Caching
alone therefore does not solve the requested diagnostic-recording throughput.

## Bounded live recording and storage reserve

Live camera video now uses separate bounded encoder workers. If an encoder
cannot keep up, that optional stream stops accepting images, drains its accepted
FIFO prefix and retains a finalized `.partial` container. Numeric/quality
recording and live outputs continue. The manifest distinguishes `complete`,
`stopped_overload` and `failed`, with submitted/encoded counts and source-frame
and timestamp bounds. A partial prefix never counts as a completed video.
Recorded-file/headless processing retains lossless backpressure instead.

Each stream allows eight queued images and one in flight, with a 32 MiB budget
covering pending payload bytes. The initial two-image queue passed the numeric
continuity checks but stopped some four-camera MJPEG raw and uncompressed debug
streams during startup, including a raw encoder averaging 158 FPS. Those failed
full-video cases remain explicit prior-candidate evidence. Eight queued images
provide about 90 ms of burst headroom at 100 FPS, while retaining the byte limit
and explicit overload policy.

Source-clock gap reports include the first recorded second and control
transitions, including any initial pause/resume or retained starting row. Their
rounded nominal missing-interval counts are diagnostics, rather than verified
sensor losses or a causal attribution to a selected recording mode. Steady
processed rate, complete all-frame evidence and video completeness are reported
separately.

On Linux live recordings, a best-effort free-space check runs at worker startup
and at most every 100 ms. Falling below a 512 MiB reserve, or being unable to
check space, fails only that optional stream and retains partial evidence when
finalization remains possible. This is neither a disk quota nor a guarantee
against other writers consuming the reserve. It does not apply to offline
recording or establish equivalent behavior on other operating systems.
Injected-probe tests cover low-space/error handling without filling the shared
filesystem. The development raw-video matrix separately reserves at least
1 GiB and may remove only fully decoded/count-verified/hashed generated video
after saving its manifest, source bounds and first/last image excerpts.

## Final native matrix

The final desktop SHA256 is
`2a34a9bc2da67cf5e8f9e4678276f08905eead41b3e1e8fa3b0d007f0c0e6ec0`.
Each case uses the actual four rig calibrations, 320×240/100-FPS capture and a
30-FPS display target. H264/MJPEG cases measure four seconds after one warmup
second; uncompressed cases measure two seconds after one warmup second to bound
disk consumption. Startup samples and complete source clocks include the
transitions excluded from steady CPU/FPS timing. Real healthy, undrained and
incomplete-greeting ZMQ peers run throughout. All performance cases are serial.

| Encoding / selection | One camera numeric FPS | Four-camera numeric FPS | Four-camera CPU | Requested video result |
|---|---:|---:|---:|---|
| Data only, raw matrix | 100.85 | 100.47–100.96 | 186.6% | Numeric/quality complete |
| Configured H264 / debug | 101.44 | 100.62–101.12 | 197.3% | Explicit debug prefixes |
| MJPEG / raw | 101.42 | 100.64–101.64 | 315.8% | All raw videos complete |
| MJPEG / debug | 101.68 | 100.21–101.20 | 206.4% | Explicit debug prefixes |
| MJPEG / both | 100.93 | 100.39–101.14 | 320.6% | Raw complete; debug prefixes |
| Uncompressed / raw | 101.11 | 100.48–100.97 | 208.9% | All videos complete |
| Uncompressed / debug | 101.35 | 100.52–102.50 | 463.0% | All videos complete |
| Uncompressed / both | 99.83 | 100.31–102.28 | 491.2% | All videos complete |

CPU for overloaded cases mostly measures continued numeric processing after
the optional encoder has stopped. It is not the cost of successful ongoing
diagnostic encoding. Uncompressed debug/both CPU measures active complete video
recording; values above 400% use the machine's eight logical CPUs. The short
sampling window and asynchronously published counters explain small rate
variation around the requested 100 FPS; no indefinite throughput claim follows.

Every final case meets the declared 95 processed-FPS threshold. The raw matrix
retains 6,484 complete numeric/quality rows and 6,471 accepted rows, all matching
actual FT bytes; all 20 requested videos fully decode with exact row counts and
source-prefix identities. Across all final matrices there are 22,280 all-frame
rows, 22,266 accepted rows received exactly, 30 complete video streams and 15
explicit finalized overload prefixes. Rejected rows remain visible in numeric
and quality files. Live content varies, so these quality decisions do not
replace the fixed-video accuracy benchmark.

All four uncompressed **both-video** takes have maximum source-clock gaps at
most 12.395 ms and zero rounded excess-interval estimates, matching their data
baseline's normal 8/12-ms cadence. Other recorded control/startup windows retain
larger gaps: maxima reach 36.014 ms for uncompressed debug, 40.000 ms for H264
debug and 59.989 ms for MJPEG both. Their per-rig estimates and first-second
counts remain in the report. These observations include initial/control
conditions and establish neither sensor-level zero loss nor a causal recording
mode attribution.

The test removes only verified generated uncompressed movie bytes after saving
hashes, decoded counts, final manifests and first/last PNGs. Full original VR3
recordings stay untouched. The positive short-clip result and the Linux reserve
guard do not remove the need for enough sustained storage bandwidth and space.

Reproduce the phase profile with:

```sh
cargo build --release --locked -p fictrac-io --example profile_recording
target/release/examples/profile_recording \
  runs/exposure-vr3/original-snapshot VR3.txt runs/recording-phase-new 20
```
