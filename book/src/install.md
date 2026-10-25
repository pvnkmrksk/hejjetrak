# Download and install

[**Download the latest release**](../#download) or [choose a package on GitHub](https://github.com/pvnkmrksk/hejjetrak/releases/latest). The download page reads the latest published release and offers every available platform explicitly.

Published **v0.1.2** supplies four portable archives. The additional Debian/Ubuntu `.deb` belongs to **v0.1.3 development** and is offered only when an asset is actually present in the selected published release.

Choose a platform download, **not** GitHub's “Source code” ZIP. No compiler or Rust installation is needed. Portable archives need no system installation; the optional Debian/Ubuntu installer uses your package manager.

| Computer | Archive | Launch |
|---|---|---|
| Mac with Apple Silicon (M-series) | `hejjetrak-macos-arm64.zip` | `hejjetrak.app` |
| Mac with Intel processor | `hejjetrak-macos-x86_64.zip` | `hejjetrak.app` |
| Windows x64 | `hejjetrak-windows-amd64.zip` | `hejjetrak.exe` |
| Debian / Ubuntu amd64 | `hejjetrak-linux-amd64.deb` when present | App menu → hejjetrak |
| Linux x86_64 portable | `hejjetrak-linux-x86_64.tar.gz` | `hejjetrak` |

Keep the extracted folder together: it contains example videos, configuration and notices. On Mac, those resources live inside the app bundle. The companion command-line executable is `hejjetrak-cli` (`hejjetrak-cli.exe` on Windows), inside `hejjetrak.app/Contents/MacOS` on Mac. The GUI needs no terminal command.

## macOS

1. Extract the ZIP in Finder. Move `hejjetrak.app` to Applications or another writable location if desired.
2. Double-click the app. Allow camera access when you want to use a camera.
3. The Mac app is **ad hoc signed, not Developer ID signed or notarized**. If macOS blocks its first launch because the developer cannot be verified, first verify that you downloaded it from this project's GitHub release. After attempting to open it, open **System Settings → Privacy & Security → Open Anyway**, then confirm **Open** if you trust this particular app.
4. Future launches can use double-click normally. Managed Macs may prohibit this exception; ask your administrator in that case.

These steps use Apple's per-application exception, described in [Safely open apps on your Mac](https://support.apple.com/en-us/102445). They do not require disabling Gatekeeper, removing quarantine attributes or changing global security settings. If macOS says the app is damaged or contains malware, stop and investigate the exact warning rather than treating it as an ordinary unidentified-developer alert.

To find your chip, use **Apple menu → About This Mac**. A browser may identify macOS without revealing whether its CPU is Apple Silicon or Intel; the download page lets you choose explicitly.

## Windows

1. Extract the ZIP fully; do not run the executable from inside the ZIP preview.
2. Double-click `hejjetrak.exe`. This executable is unsigned. Windows may show an unknown-publisher/reputation warning.
3. If you trust the verified release and the dialog offers it, choose **More info → Run anyway** for this app. Organizational policy may remove that option. Do not disable SmartScreen or antivirus protection.
4. Allow camera access in Windows privacy settings if needed.

Microsoft explains that [SmartScreen checks downloads and app reputation](https://learn.microsoft.com/en-us/windows/security/operating-system-security/virus-and-threat-protection/microsoft-defender-smartscreen/). A reputation warning does not establish that software is safe. The x64 release uses Rust's static CRT linkage; a separate Visual C++ redistributable is not required by the audited v0.1.1 executables.

**Smart App Control is a different Windows protection.** It can block unsigned code without offering an individual-app exception. If that is the block shown, keep protection enabled and contact the maintainer/administrator about an approved signed build; this guide does not instruct you to turn it off. See [Microsoft's Smart App Control FAQ](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions).

## Linux

### Debian / Ubuntu installer

Choose **Debian / Ubuntu · .deb** on the download page when that asset exists in the selected release. Open the downloaded package in your distribution's graphical package installer. It can request administrator authentication to install system files and declared dependencies. Then launch **hejjetrak** from the app menu; the CLI is available as `hejjetrak-cli` on PATH.

Alternatively, from the download directory:

```sh
sudo apt install ./hejjetrak-linux-amd64.deb
```

That command installs the package; never run the tracking app with `sudo`. Use the portable archive below if you want no system installation. The package installs resources under `/usr/lib/hejjetrak`; copy rig/example configs into your own writable experiment folder before editing/saving them. Your recordings/configs stay separate from managed application files. Remove the installed application through the package manager (`sudo apt remove hejjetrak`); preserve your experiment files independently.

The installer declares actual linked ABI requirements detected from the release binaries plus the desktop, graphics-loader and camera libraries loaded at runtime. It recommends a desktop portal for the file chooser. Graphics drivers, a running graphical session and device permissions still belong to your distribution. Builds use the release's Ubuntu runner: an older Debian/Ubuntu may not satisfy its C-library versions. Do not bypass dependency checks; the portable archive uses the same binary ABI and is not a fix for an older system.

### Portable archive

Extract the tar archive with your archive manager, then run `hejjetrak`. A graphical desktop with a working Vulkan/OpenGL-capable graphics driver is needed for the native GUI. Standard desktop and camera system libraries are not bundled; distributions differ. The Linux archive is built on a GitHub-hosted Ubuntu runner; compatibility with an older distribution's C library/graphics stack is not established.

If your file manager asks whether to execute the file, use its **Allow executing file as program** control. The archive normally preserves executable permissions. From a terminal in the extracted folder:

```sh
./hejjetrak
```

If the executable bit was lost during copying, `chmod u+x hejjetrak hejjetrak-cli` repairs the permissions on just those files. No root access is needed for extraction or ordinary launch. Camera access follows the distribution's device permissions and desktop privacy policy. Do not solve permission errors by running the application as root.

## Updates and checksums

Download a new release into a new folder and keep your experiment configuration and recordings separately. Read the release notes before changing a working experiment. GitHub's release API may provide an asset SHA-256 digest; the download dialog shows it when present. Compare it with a locally calculated digest when verifying an archive. Never overwrite your only copy of a calibrated config or run output during an update.
