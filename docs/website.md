# Website and manual maintenance

The landing page is `site/`, the mdBook manual is `book/`, and shared technical reports stay canonical in `docs/`. The site organization was informed by [Rekkefly](https://github.com/pvnkmrksk/rekkefly)'s separate landing page and chapter manual; no Rekkefly implementation, assets, measurements or fonts were copied. Attributed upstream FicTrac setup, parameter, coordinate and data-field documentation is preserved in the manual under CC BY-NC-SA 3.0.

The site is static. No application runtime depends on the website, Python, mdBook, Node, JavaScript or GitHub's API. The landing page uses system fonts; mdBook serves its bundled fonts/assets locally. There are no analytics, external font services or JavaScript frameworks.

## Build and verify

Use official **mdBook 0.5.4**, then:

```sh
node scripts/test_downloads.mjs
python3 scripts/build_site.py /tmp/hejjetrak-site --mdbook /path/to/mdbook
python3 -m http.server 8000 --directory /tmp/hejjetrak-site
```

The build stages canonical reports and shared images into the manual, invokes mdBook, adds visible Website and GitHub links to every chapter toolbar (including the combined print page), then checks local HTML links, assets and fragment targets. These links are static HTML and remain available when JavaScript is disabled. The no-JavaScript sidebar iframe remains a table of contents. Toolbar replacement is checked against the pinned mdBook template; a missing template target fails the build. It refuses to replace an unmarked existing output directory. The generated tree is disposable. The Pages workflow repeats selection checks and link checks before uploading/deploying it.

Verify both wide and narrow screens, keyboard navigation, the manual/search, and a real release download flow. Manual platform choices must remain visible after an automatic download. On Safari, `MacIntel` in the user agent does **not** establish Intel hardware; the automatic flow must explain its Apple Silicon default and offer Mac Intel beside it.

## Download behavior

Download actions read `https://api.github.com/repos/pvnkmrksk/hejjetrak/releases/latest`. Only an exact known package/archive name and an HTTPS asset URL under this repository's GitHub release-download path are accepted. Current assets use `hejjetrac-*` filenames; the selector also recognizes the corresponding `hejjetrak-*` files from older releases and prefers the current name when both are present. Release version, archive size and a SHA-256 digest are shown when available. Assets are never inferred from an unreleased source revision. A missing platform during a partial release upload produces an explicit message instead of a guessed URL. API errors and JavaScript-disabled browsers retain links to the ordinary GitHub release page.

The automatic flow uses a reliable browser architecture hint when available. A Mac without a reliable chip hint defaults to Apple Silicon and displays that assumption with an immediate Mac Intel alternative. Linux defaults to the portable x86_64 archive; the Debian/Ubuntu installer remains available alongside it. Windows defaults to x64. Unsupported ARM or mobile hints do not silently select a desktop x64 binary, and an unrecognized platform opens the manual choices. Manual platform buttons retain the Mac chip and Linux package choices. A download displays first-launch instructions appropriate to the selected OS. macOS ad hoc signing is never described as Developer ID signing or notarization. The instructions use Apple's per-app exception, do not remove quarantine, and do not disable Gatekeeper, SmartScreen or antivirus. Linux users choose the Debian/Ubuntu `.deb` installer or portable `.tar.gz`; a missing installer asset is reported explicitly. Installing the system package may need authentication; portable extraction and ordinary launch do not. Both use distribution desktop/graphics libraries and native device permissions. Mac labels read Apple Silicon and Mac Intel. The repository and Pages paths remain `/hejjetrak/` so existing links stay valid; executable names use `hejjetrac`, with the older command names retained as aliases.

The README’s **Download now** link opens `https://pvnkmrksk.github.io/hejjetrak/?download=1`. That page immediately opens the platform guide, resolves the default package and triggers its archive link. It displays the selected release, first-launch instructions, a Download again button and other platform choices. The query is consumed with browser history replacement so a refresh does not repeatedly trigger the download. GitHub Markdown itself cannot run OS detection; it displays the animated GIF and links to this website flow. Browser autoplay and download policies may require a user click, so visible controls and the GitHub Releases fallback remain available.

## Publish

The repository must use **Settings → Pages → Source: GitHub Actions**. `.github/workflows/pages.yml` builds on relevant main pushes and PRs; only main or manual main dispatch deploys. If Pages is not configured, it emits a notice and retains the build artifact without pretending publication succeeded.

Expected URL: `https://pvnkmrksk.github.io/hejjetrak/`, with manual under `/manual/`. If the repository or custom domain changes, update the landing page canonical/social metadata, mdBook `site-url`, README, the 404 paths and workflow URL together. External links and actual public reachability should be verified after deployment; a successful local build is not publication evidence.

## Screenshots and releases

`docs/images/preview.png` and `diagnostics.png` are actual native screenshots of the attributed sample, never private live-camera frames. `tracking-demo.mp4` and `tracking-demo.gif` capture the native sample playback; only the OS title bar is cropped, then frames are scaled and encoded for the web. The website video uses `autoplay loop muted playsinline`, with a silent source and a still poster. Its script attempts playback on opening, keeps the play/pause control synchronized and leaves native controls available when the browser blocks autoplay. The GIF loops in GitHub Markdown where the viewer permits animation; GitHub does not support forcing video autoplay. Capture a fresh screenshot after UI changes, visually inspect it, and update the manual/README alt text if the view changes. Do not use mockups as runtime evidence.

The manual follows main, so installation/release chapters must distinguish published features from unreleased source changes. The latest release asset is selected dynamically, while historical benchmark numbers remain explicitly tied to their recorded fixture/build. Do not turn a single sample timing into a sixteen-camera or general energy-use claim.

The landing page follows the system light/dark preference and uses system fonts. Keyboard focus, native download dialogs, manual platform selection and reduced-motion behavior remain available. Closing a download dialog cancels any pending automatic download before the archive link has been triggered. The source site remains hosted through the existing GitHub Pages workflow.
