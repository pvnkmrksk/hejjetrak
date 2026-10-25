# Download and install

[**Download the latest release**](../?download=1) or [choose a package on GitHub](https://github.com/pvnkmrksk/hejjetrak/releases/latest). The download page reads the latest published release and offers every available platform explicitly.

Release packages include four portable archives and a Debian/Ubuntu `.deb`. The download page offers only assets present in the selected published release. The package names below use the current `hejjetrac` spelling; v0.1.3 and earlier use `hejjetrak` names. Old commands remain available as compatibility aliases in new packages. The Linux binaries require **glibc 2.39 or newer**; see the distribution requirements below.

Choose a platform download, **not** GitHub's “Source code” ZIP. No compiler or Rust installation is needed. Portable archives need no system installation; the optional Debian/Ubuntu installer uses your package manager.

| Computer | Archive | Launch |
|---|---|---|
| Mac with Apple Silicon (M-series) | `hejjetrac-macos-arm64.zip` | `hejjetrac.app` |
| Mac with Intel processor | `hejjetrac-macos-x86_64.zip` | `hejjetrac.app` |
| Windows x64 | `hejjetrac-windows-amd64.zip` | `hejjetrac.exe` |
| Debian / Ubuntu amd64 | `hejjetrac-linux-amd64.deb` when present | App menu → HejjéTrac |
| Linux x86_64 portable | `hejjetrac-linux-x86_64.tar.gz` | `hejjetrac` |

Keep the extracted folder together: it contains example videos, configuration and notices. On Mac, those resources live inside the app bundle. The companion command-line executable is `hejjetrac-cli` (`hejjetrac-cli.exe` on Windows), inside `hejjetrac.app/Contents/MacOS` on Mac. The GUI needs no terminal command.

## macOS

1. Extract the ZIP in Finder. Move `hejjetrac.app` to Applications or another writable location if desired.
2. Double-click the app. Allow camera access when you want to use a camera.
3. The Mac app is **ad hoc signed, not Developer ID signed or notarized**. If macOS blocks its first launch because the developer cannot be verified, first verify that you downloaded it from this project's GitHub release. After attempting to open it, open **System Settings → Privacy & Security → Open Anyway**, then confirm **Open** if you trust this particular app.
4. Future launches can use double-click normally. Managed Macs may prohibit this exception; ask your administrator in that case.

These steps use Apple's per-application exception, described in [Safely open apps on your Mac](https://support.apple.com/en-us/102445). They do not require disabling Gatekeeper, removing quarantine attributes or changing global security settings. If macOS says the app is damaged or contains malware, stop and investigate the exact warning rather than treating it as an ordinary unidentified-developer alert.

To find your chip, use **Apple menu → About This Mac**. A browser may identify macOS without revealing whether its CPU is Apple Silicon or Intel. The automatic download uses Apple Silicon as its stated default in that case; choose **Mac · Intel** in the same dialog for an Intel Mac.

## Windows

1. Extract the ZIP fully; do not run the executable from inside the ZIP preview.
2. Double-click `hejjetrac.exe`. This executable is unsigned. Windows may show an unknown-publisher/reputation warning.
3. If you trust the verified release and the dialog offers it, choose **More info → Run anyway** for this app. Organizational policy may remove that option. Do not disable SmartScreen or antivirus protection.
4. For a camera, open **Settings → Privacy & security → Camera** and enable **Camera access** and **Let desktop apps access your camera**. See [Microsoft's camera permissions guide](https://support.microsoft.com/en-us/windows/privacy/manage-app-permissions-for-a-camera-in-windows).

Microsoft's [app publishing guide](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/publish-first-app#step-6-handle-smartscreen-for-new-apps) describes the first-run SmartScreen prompt. The x64 release uses Rust's static CRT linkage; a separate Visual C++ redistributable is not required by the audited v0.1.1 executables.

**Smart App Control is a different Windows protection.** It can block unsigned code without offering an individual-app exception. If that is the block shown, keep protection enabled and contact the maintainer/administrator about an approved signed build; this guide does not instruct you to turn it off. See [Microsoft's Smart App Control FAQ](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions).

## Linux

### Debian / Ubuntu installer

Choose **Debian / Ubuntu · .deb** on the download page when that asset exists in the selected release. Open the downloaded package in your distribution's graphical package installer. It can request administrator authentication to install system files and declared dependencies. Then launch **HejjéTrac** from the app menu; the CLI is available as `hejjetrac-cli` on PATH.

Alternatively, from the download directory:

```sh
sudo apt install ./hejjetrac-linux-amd64.deb
```

That command installs the package; never run the tracking app with `sudo`. Use the portable archive below if you want no system installation. The package installs resources under `/usr/lib/hejjetrak`; copy rig/example configs into your own writable experiment folder before editing/saving them. Your recordings/configs stay separate from managed application files. Remove the installed application through the package manager (`sudo apt remove hejjetrak`); preserve your experiment files independently.

The installer declares actual linked ABI requirements detected from the release binaries plus the desktop, graphics-loader and camera libraries loaded at runtime. It recommends a desktop portal for the file chooser. Graphics drivers, a running graphical session and device permissions still belong to your distribution. The v0.1.3 build uses Ubuntu 24.04 and requires **glibc 2.39 or newer**. Ubuntu 22.04 and Debian 12 do not meet that requirement. Do not bypass dependency checks; the portable archive uses the same binary ABI and is not a fix for an older system. Other distributions still need the listed desktop/camera libraries and working graphics drivers.

### Portable archive

Extract the tar archive with your archive manager, then run `hejjetrac`. A graphical desktop with a working Vulkan/OpenGL-capable graphics driver is needed for the native GUI. Standard desktop and camera system libraries are not bundled; distributions differ. The Linux archive is built on a GitHub-hosted Ubuntu runner; compatibility with an older distribution's C library/graphics stack is not established.

If your file manager asks whether to execute the file, enable its **Executable as Program** or **Allow executing file as program** permission. In GNOME Files, this is under **Properties → Permissions**; see [GNOME's file permissions guide](https://help.gnome.org/gnome-help/nautilus-file-properties-permissions.html). The archive normally preserves executable permissions. From a terminal in the extracted folder:

```sh
./hejjetrac
```

If the executable bit was lost during copying, `chmod u+x hejjetrac hejjetrac-cli` repairs the permissions on just those files. No root access is needed for extraction or ordinary launch. Camera access follows the distribution's device permissions and desktop privacy policy. Do not solve permission errors by running the application as root.

## Updates and checksums

Download a new release into a new folder and keep your experiment configuration and recordings separately. Read the release notes before changing a working experiment. GitHub's release API may provide an asset SHA-256 digest; the download dialog shows it when present. Compare it with a locally calculated digest when verifying an archive. Never overwrite your only copy of a calibrated config or run output during an update.

## Download size

The package includes the native GUI and graphics integration, fonts, Rust video
codecs and tracking engine, plus the companion CLI, sample video, examples and
notices. Much of that code is linked into the executables so you can extract and
open the app without installing Rust or a separate media runtime. It is shared
overhead: an app with fewer controls still needs a window, renderer and decoder.
Size therefore does not grow in proportion to the number of features. The exact
download size varies by platform and release and appears in the download dialog.
