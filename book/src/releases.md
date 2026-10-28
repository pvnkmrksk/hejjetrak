# Releases and downloads

[**Download the latest published release**](../?download=1) · [All releases and notes](https://github.com/pvnkmrksk/hejjetrak/releases)

The landing page resolves GitHub's latest published release at download time and shows the platform archive, release tag and available SHA-256 digest. Manual OS/chip choices remain available; browser architecture detection is not treated as infallible. If the release API is unavailable or an archive has not uploaded yet, use the visible GitHub release link rather than a guessed download.

## v0.3.4 — 28 October 2026

[v0.3.4](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.3.4) retains the
v0.3.3 tracking, networking and Linux graphics changes and corrects portable
busy-port test setup. Test reservations now match the wildcard publisher bind,
so native checks exercise a real endpoint conflict on macOS as well as Linux
and Windows. Numeric reference tolerances are unchanged.
[Release notes](https://github.com/pvnkmrksk/hejjetrak/blob/v0.3.4/docs/release-v034.md).

## v0.3.3 — 28 October 2026

[v0.3.3](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.3.3) includes
more robust fast-motion/partial-atlas fitting and capture-interval prediction,
restored threshold controls, ZeroMQ enabled by default for localhost and IPv4
LAN clients, and Linux software graphics mode with persistent diagnostics.
Existing explicit output settings and saved rig ports remain authoritative.
[Release notes](https://github.com/pvnkmrksk/hejjetrak/blob/v0.3.3/docs/release-v033.md)
retain fixture-specific tracking evidence and outstanding limitations. Platform
packages appear as their release jobs complete; offer only downloads actually
present in the release.

## v0.3.2 — 27 October 2026

[v0.3.2](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.3.2) is published
with all five packages. Linux X11/XWayland startup prefers compatible OpenGL and
automatically uses Vulkan when OpenGL cannot present to the window. Ordinary
startup needs no backend override. Explicit graphics choices remain respected;
tracking and camera settings are unchanged.

All native build/test jobs pass. The same installed and portable packages pass
normal GL and automatic Vulkan rendering on Ubuntu 22.04/24.04, including four
forced-GL failure controls and ten inspected previews. These are Xvfb/Mesa
software-graphics checks. [Validation](validation.md#v032-published-release--2026-10-27)
records hashes, source revisions and remaining platform limits.

## v0.3.1 — 27 October 2026

[v0.3.1](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.3.1) is published
with all five platform packages. Native platform checks, official archive audits
and actual installed/portable Ubuntu 22.04/24.04 tracking/preview checks pass.
[Validation](validation.md#v031-published-release--2026-10-27) records source,
package hashes and the separately verified Debian-container correction.

This release combines the four-camera Linux improvements with v0.2.3's saved
user configuration and recording-clock fixes.

- More reliable fast-motion fitting and spatial atlas retention.
- Linux camera timestamps, camera ownership checks and native display fixes.
- Independent display FPS and recording speed/size controls.
- Bounded background video encoding and live recording finalization, with
  explicit completion markers and partial-media status.
- Saved-layer calibration review and a simpler daily toolbar.
- Direct LAN publishers and a four-rig launch helper for existing UDP bridges.

The included VR1–VR4 files describe one particular calibrated 320×240/100-FPS
setup. Check camera IDs, formats, output ports and geometry for your own rig.
The [recording guide](recording.md) and [validation record](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/validation.md)
retain the tested limits and historical overnight-run evidence.


The v0.3.0 candidate remains unpublished. v0.3.1 includes the corrected
build-time timezone setup and dependency-notice collector, including exact
source provenance for legacy crates and complete bundled font terms.

## v0.2.3 — 26 October 2026

[v0.2.3](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.2.3) was published with all five packages. All native build and Ubuntu runtime
checks pass. The official Mac archive passes signature, complete-sample,
preview and saved-default relaunch checks. [Validation](validation.md#v023-published-release--2026-10-26)
records hashes and execution scope.

Desktop recordings prefer an explicit `src_fps`, then the source's reported
frame rate. Capture jitter in the first two frames no longer changes the whole
video's playback speed. Numeric and quality timestamps remain unchanged;
constant-rate video still approximates capture jitter and gaps. A single-frame
take also uses the known source rate. Normal launch keeps a writable default
config in the user's config folder and reloads its saved source and calibration.
Installed templates remain unchanged; an unavailable config folder falls back
to a temporary preview with Save As guidance.

## v0.2.2 — 26 October 2026

[v0.2.2](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.2.2) is published
with all five platform packages. All four native package jobs and both Ubuntu
runtime jobs pass. The official Apple Silicon archive passes strict ad hoc
signature, version, source and complete sample checks; its native preview was
visually checked. [Validation](validation.md#v022-published-release--2026-10-26)
records exact package hashes, test scope and remaining limits.

Linux packages now target Ubuntu **22.04 and newer**, with a checked glibc 2.35
ceiling. The same installed and portable builds are tested on Ubuntu 22.04 and
24.04 with the full sample and a visible native preview. Camera startup handles
monochrome `GREY` and MJPEG formats, bounds frame waits, and keeps the preview
and recording available when an output port is occupied. The Mac capture
adapter is unchanged; physical Linux camera testing remains outstanding.

Fast-motion recovery retains a valid acceleration candidate as a search
proposal instead of repeatedly starting from a stationary guess. Quality
thresholds, confirmation rules and missing-motion semantics remain unchanged.
Sharp and blurred known-motion regressions and the existing real-video
recovery oracle cover the change. [Recovery](recovery.md) explains the limits.

The minor release adds explicit `legacy_matrex_vr` JSON output and staged Rust
ZMTP 2/3 negotiation for the original NetMQ 4.0.0.1 DLL. Native FT remains the
default. Single JSON matches the Python republisher; multipart topic/JSON supports
paired-read receivers. Mode, framing, topic and port are available in Settings.

Unchanged v0.1.0 and v0.3.0 Mac players pass the functional movement, heading,
reconnect and repeated scene-reload matrix. The old v0.1.0 receiver retains its
worker-stop warnings and strict lifecycle acceptance remains false; see
[the complete compatibility scope](matrex-vr.md). The new transport requires no
Python or native libzmq runtime. Full release execution evidence is recorded in
[validation](validation.md).

The v0.2.0 and v0.2.1 candidates remain unpublished. Windows CI exposed a
test-peer shutdown race, corrected by retaining the test connection through its
validation assertion. Subsequent Ubuntu and tracking fixes are included here;
the candidate tags and CI evidence are retained.

**Known recording limitation:** on a live camera, the desktop can select video
recording FPS from a jittered first frame interval. A 9 ms interval produced
111.111 FPS video metadata for an approximately 120 FPS source. Numeric
timestamps remain intact. Use those timestamps for capture timing; a correction
is under investigation for a later patch. The live tracking check also needs a
corrected ball-only calibration before it can support an accuracy claim.

## v0.1.4 — 25 October 2026

[v0.1.4](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.4) introduces the **HejjéTrac** display name and `hejjetrac` commands. Calibration opens directly into a fresh ball-outline draft, fits the outline automatically from the third point, and never joins rim marks with straight lines. Diagnostics includes the complete saved calibration, direct layer redraw and full config access. The website starts its silent demo automatically and the README download link opens the platform download page directly.

All five platform packages were published. All four [native release jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40198640739) pass: 188 workspace tests on Linux and both Macs, 185 on Windows, and 1,353 decoder tests each. The official Mac archive passes signature, version, source-commit and complete sample checks. A public README click completed the recommended v0.1.4 download without a second click. See [validation evidence](validation.md) for hashes, execution scope and remaining hardware/visual limits.

## Earlier published checkpoint

The preceding checkpoint **[v0.1.3](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.3)** has five assets: Mac Apple Silicon and Intel ZIPs, Windows x64 ZIP, Linux x64 portable tar archive and Linux amd64 `.deb`. All four [native release jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40180700262) passed: Linux and both Macs ran 173 workspace tests, Windows ran 170 platform-applicable tests, and all four ran 1,353 decoder tests. The official Apple Silicon archive passed strict signature/version checks, the complete sample comparison with unchanged reference gates, and native preview/recording inspection. Native hover/loupe pointer testing remains incomplete. The download dialog reads the actual latest published release; a source change is included in a binary only after its release has been built and uploaded. Consult the selected release tag's README and notes when reproducing a build. A dynamically selected latest archive always follows the published release, not an arbitrary CI artifact.

Each platform archive contains the native desktop executable, CLI companion, sample/config resources, documentation, upstream license and dependency notices. Mac archives contain an app bundle. Linux retains its portable tar archive and v0.1.3 additionally offers an amd64 Debian/Ubuntu `.deb` with an app-menu icon and explicit system-library dependencies. Mac/Windows use ZIP. Current packages use `hejjetrac` archive and executable names, with `hejjetrak` command aliases for existing scripts; the download page never invents a `.deb` URL for a release without that asset. No package is a signed store installer.

## Release integrity and opening

macOS packages are ad hoc signed, not Developer ID signed or notarized; Windows executables are unsigned. See the per-platform [first-launch instructions](install.md). Portable extraction and ordinary use require no administrator access. Installing/removing the optional Linux system package can require package-manager authentication. GitHub Actions packages native Windows x64, Linux x64 and Mac Apple Silicon/Intel builds. Native CI tests and a synthetic CLI run do not establish camera-hardware or GUI visual coverage on every OS.

## History

- **v0.1.0:** first native Rust proof of concept, legacy config/data, CLI/agent surfaces and sample/known-motion regressions.
- **v0.1.1:** compact native previews, separate config inspector, camera capabilities/format controls, simultaneous localhost ZMQ and UDP, tiled instances and packaging repairs. The Windows archive was rebuilt from the unchanged source tag with static CRT linkage.
- **v0.1.2:** bounded decode/fit overlap, immutable shared frames, Rust decoder fixes and accelerated local candidate search; command-interruptible waits and coalesced previews; guided calibration/magnifier and explicit resolution references; protected atlas recovery after unreliable frames. See measured [pipeline](performance.md), [tracking](tracking-performance.md), validation and the repository commit history. All four native release jobs passed, and the official Mac archive passed signature, sample and native visual smoke checks.

- **v0.1.3:** read-only Keep/Edit/Redraw calibration review, a larger
  precision loupe, image-only tiny previews, concise applied-source titles,
  independent per-window close, bundled Lucide icons and stronger recovery
  learning safeguards. Linux retains the portable archive and adds an optional
  Debian/Ubuntu package. All four native release jobs passed; official Apple
  Silicon archive checks and sample comparison passed. Native hover/loupe mouse
  interaction remains incompletely verified.

## Building and publishing

Tag-triggered GitHub Actions builds platform archives and attaches them as jobs finish. Until all uploads complete, a release can have only some platforms. The download page offers only assets present in the API response and exposes all releases as fallback. Source tags remain immutable; fixes should use a new release or an explicitly documented artifact rebuild.
