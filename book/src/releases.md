# Releases and downloads

[**Download the latest published release**](../?download=1) · [All releases and notes](https://github.com/pvnkmrksk/hejjetrak/releases)

The landing page resolves GitHub's latest published release at download time and shows the platform archive, release tag and available SHA-256 digest. Manual OS/chip choices remain available; browser architecture detection is not treated as infallible. If the release API is unavailable or an archive has not uploaded yet, use the visible GitHub release link rather than a guessed download.

## v0.2.2 — Ubuntu compatibility, fast-motion recovery and MATREX VR (upcoming)

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

## v0.1.4 — 25 October 2026

[v0.1.4](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.4) introduces the **HejjéTrac** display name and `hejjetrac` commands. Calibration opens directly into a fresh ball-outline draft, fits the outline automatically from the third point, and never joins rim marks with straight lines. Diagnostics includes the complete saved calibration, direct layer redraw and full config access. The website starts its silent demo automatically and the README download link opens the platform download page directly.

All five platform packages are published and v0.1.4 is the latest release. All four [native release jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40198640739) pass: 188 workspace tests on Linux and both Macs, 185 on Windows, and 1,353 decoder tests each. The official Mac archive passes signature, version, source-commit and complete sample checks. A public README click completes the recommended download without a second click. See [validation evidence](validation.md) for hashes, execution scope and remaining hardware/visual limits.

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
