# Releases and downloads

[**Download the latest published release**](../#download) · [All releases and notes](https://github.com/pvnkmrksk/hejjetrak/releases)

The landing page resolves GitHub's latest published release at download time and shows the platform archive, release tag and available SHA-256 digest. Manual OS/chip choices remain available; browser architecture detection is not treated as infallible. If the release API is unavailable or an archive has not uploaded yet, use the visible GitHub release link rather than a guessed download.

## Published vs source behavior

The manual follows **v0.1.2 source** on main. The download dialog reads the actual latest published release; a source change is included in a binary only after its release has been built and uploaded. Consult the selected release tag's README and notes when reproducing a build. A dynamically selected latest archive always follows the published release, not an arbitrary CI artifact.

Each platform archive contains the native desktop executable, CLI companion, sample/config resources, documentation, upstream license and dependency notices. Mac archives contain an app bundle. Linux uses a tar archive to preserve executable bits; Mac/Windows use ZIP. These are portable distributions, not signed store installers.

## Release integrity and opening

macOS packages are ad hoc signed, not Developer ID signed or notarized; Windows executables are unsigned. See the per-platform [first-launch instructions](install.md). Ordinary extraction and use require no administrator access. GitHub Actions packages native Windows x64, Linux x64 and Mac Apple Silicon/Intel builds. Native CI tests and a synthetic CLI run do not establish camera-hardware or GUI visual coverage on every OS.

## History

- **v0.1.0:** first native Rust proof of concept, legacy config/data, CLI/agent surfaces and sample/known-motion regressions.
- **v0.1.1:** compact native previews, separate config inspector, camera capabilities/format controls, simultaneous localhost ZMQ and UDP, tiled instances and packaging repairs. The Windows archive was rebuilt from the unchanged source tag with static CRT linkage.
- **v0.1.2 source:** bounded decode/fit overlap, immutable shared frames, Rust decoder fixes and accelerated local candidate search; command-interruptible waits and coalesced previews; guided calibration/magnifier and explicit resolution references; protected atlas recovery after unreliable frames. See measured [pipeline](performance.md), [tracking](tracking-performance.md), validation and the repository commit history. The download dialog identifies whether a published v0.1.2 binary is available.

## Building and publishing

Tag-triggered GitHub Actions builds platform archives and attaches them as jobs finish. Until all uploads complete, a release can have only some platforms. The download page offers only assets present in the API response and exposes all releases as fallback. Source tags remain immutable; fixes should use a new release or an explicitly documented artifact rebuild.
