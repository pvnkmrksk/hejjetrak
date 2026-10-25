# HejjéTrac v0.1.4 release notes

Published on 25 October 2026 from `eed74b17dd29ed551140eab0d90c28c570308fcf`.
[Download v0.1.4](https://github.com/pvnkmrksk/hejjetrak/releases/tag/v0.1.4).

The display name is **HejjéTrac**; executable and download names use
`hejjetrac`. The repository and website remain at
[pvnkmrksk/hejjetrak](https://github.com/pvnkmrksk/hejjetrak) and
[pvnkmrksk.github.io/hejjetrak](https://pvnkmrksk.github.io/hejjetrak/).

The previous public v0.1.3 release was built from
`fe56dab81677ae44959c0460ea878eceb3c7cbff`. The subsequent local polish build
was also labeled 0.1.3 and had not replaced those public downloads. Version
0.1.4 gives the calibration and naming updates a distinct release identity.

## Packages

| Platform | Download | Launcher |
|---|---|---|
| macOS Apple Silicon | `hejjetrac-macos-arm64.zip` | `hejjetrac.app` |
| macOS Intel | `hejjetrac-macos-x86_64.zip` | `hejjetrac.app` |
| Windows x64 | `hejjetrac-windows-amd64.zip` | `hejjetrac.exe` |
| Linux x86_64 portable | `hejjetrac-linux-x86_64.tar.gz` | `hejjetrac` |
| Debian/Ubuntu x64 | `hejjetrac-linux-amd64.deb` | `hejjetrac` |

The companion command is `hejjetrac-cli`. Existing `hejjetrak` and
`hejjetrak-cli` commands remain available as compatibility aliases. The Debian
package ID remains `hejjetrak`, with its files in `/usr/lib/hejjetrak`, so an
existing installation can upgrade normally. The macOS bundle identifier is
unchanged to preserve its existing OS identity and camera permissions.

Each package includes `BUILD.json` alongside its bundled documentation
(`Contents/Resources/BUILD.json` on macOS). It records version, platform,
architecture, the CI checkout commit, and SHA-256 hashes of the compiled
binaries before macOS application signing. GitHub's release-asset SHA-256 digest
describes the complete final download. Local packages without a recorded CI
checkout leave `source_commit` null.

## Release checks

The packager refuses binaries whose `--version` differs from the workspace and
refuses a macOS `Info.plist` version mismatch. Release CI verifies both the
version and commit of its source against the destination release tag before
building. A rebuild cannot attach a different source commit to an existing
release, even if both use the same version number.

The four native jobs retain formatting, strict Clippy, workspace and decoder
tests, the known-motion demo, Windows static-runtime inspection, dependency
notices, ad hoc macOS signature verification, and Linux installer validation.
All four [native release jobs](https://github.com/pvnkmrksk/hejjetrak/actions/runs/40198640739)
passed: 188 workspace tests on Linux and both Macs, 185 applicable tests on
Windows, and 1,353 decoder tests per platform. All five platform assets are
published. The official Mac archive passes signature, version, source provenance
and all 300 sample-frame checks. The public README-to-website flow downloads
v0.1.4 without a second download click. Full execution scope and remaining
platform limits are recorded in [validation.md](validation.md).
