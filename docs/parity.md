# Behavioral parity assessment — v0.3

Assessed against pvnkmrksk/fictrac commit `10fc4472af1fc9062b9bf5a92738e835b98cc3ec` and the supplied FicTrac 2.1.2 sample output. This version closes the previously missing operational features for the supported recorded-video workflow. **It does not establish unrestricted or numerical identity.**

| Capability | Implementation / evidence |
|---|---|
| Sphere tracking and six-panel display | Shared threshold/atlas fitter and actual numerical maps; native Metal screen inspected |
| Local optimizer | Pure Rust PRIMA BOBYQA; differs from upstream NLopt Powell implementation |
| Global search | Deterministic SO(3) covering and BOBYQA multistart; differs from upstream stochastic CRS2; known-pose/occlusion/large-jump/off-axis regressions |
| Templates | Real equal-area PNG confidence image import/export; matching dimensions validated atomically |
| Bad-frame/reset handling | `max_bad_frames` threshold, retained source count and integrated displacement, reset sequence/pose/path/heading, atlas retention when global enabled |
| Missing motion | Rejected frames explicitly retained in quality/all-frame data; successful relocalisation sets absolute pose with zero relative motion |
| Legacy axes and 25-column data | ROI export basis, original signs/units, principal Rodrigues orientation, independent sequence counter; supplied reference gates unchanged |
| Legacy accepted-only output | `.dat`, serial and UDP emit accepted rows; companion all-frame output prevents missing decisions from being hidden |
| Calibration | Shared rim/masks/forward/square operations; saved values precede refitting, corner-only fallback; XY/YZ/XZ and fisheye/pinhole tests |
| Config/file state | Lossless comments/unknown fields/CRLF, raw/field synchronization, history, external conflict guard; transactional agent edits |
| Multi-instance | Independent native windows and headless sessions; BR/VR selection, numeric-camera fallback, preserved video paths; four-window native agent run |
| Full CLI | Conventional commands plus legacy positional config invocation; structured status, schema, all numerical/calibration/recording operations through JSONL |
| AI agent control | JSONL and MCP stdio over shared controller; native GUI JSONL over displayed sessions; actual 299-command native run and full-video headless transcript |
| Video inputs | H.264 MP4/MOV/AVI (including B-frame ordering), MPEG-4 Part 2 MP4/MOV/AVI, MJPEG/raw AVI and mono/420 Y4M |
| Video recording | All original codec settings: H.264 AVI default, XVID AVI, MPEG-4 MP4, MJPEG/raw AVI; explicit H.264 MP4; exact FPS/container tests, calibrated debug mosaic, safe partial finalization |
| Video codec boundaries | Segmented OpenDML AVI supports >4 GiB offsets; H.264 encodes even dimensions with SPS cropping; odd H.264 4:2:0 dimensions fail explicitly; exotic profiles/formats remain outside tested coverage |
| Serial/UDP | Native Rust serial2 and datagram sinks with original prefix/full row; actual OS pseudo-terminal and loopback tests; physical serial hardware untested |
| Camera | Built-in FaceTime HD camera: actual 640×480, nominal 30 fps, 126 native GUI agent commands, pause/step/config, two 62-frame H.264 recordings, source switch and reopen; physical sphere accuracy/iPhone/industrial SDKs unverified |
| Native platforms | Mac GUI/CLI executed locally; Windows desktop and Linux CLI cross-checks pass; Linux GUI headers absent on Mac; native other-OS execution unverified |
| Packaging | Local ad hoc signed development app with dependency notices; installers/Developer ID signing/notarization/releases deferred |

A protocol request accepted into a GUI worker queue is not proof the action completed. Clients poll revision/frame/advance/recording/notice state and validate files. Synchronous headless commands expose the same numerical operations; playback and OS-window scheduling differ deliberately.

The 300-frame sample benchmark checks aligned per-frame Rodrigues deltas, path and wrapped heading with unchanged declared tolerances. Neither that reference nor synthetic renderer tests provide physical ground truth for arbitrary experiments. Numerical optimizer differences, untested hardware, codec/container limits, timing and platform differences above prevent a claim of full drop-in parity. All unsupported codec requests fail explicitly rather than silently substituting another codec.
