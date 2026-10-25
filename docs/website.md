# Website and manual maintenance

The landing page is `site/`, the mdBook manual is `book/`, and shared technical reports stay canonical in `docs/`. The site organization was informed by [Rekkefly](https://github.com/pvnkmrksk/rekkefly)'s separate landing page and chapter manual; no Rekkefly implementation, assets, measurements or fonts were copied. Attributed upstream FicTrac parameter/coordinate documentation is preserved in the manual under CC BY-NC-SA 3.0.

The site is static. No application runtime depends on the website, Python, mdBook, Node, JavaScript or GitHub's API. The landing page uses system fonts; mdBook serves its bundled fonts/assets locally. There are no analytics, external font services or JavaScript frameworks.

## Build and verify

Use official **mdBook 0.5.4**, then:

```sh
node scripts/test_downloads.mjs
python3 scripts/build_site.py /tmp/hejjetrak-site --mdbook /path/to/mdbook
python3 -m http.server 8000 --directory /tmp/hejjetrak-site
```

The build stages canonical reports and shared images into the manual, invokes mdBook, then checks local HTML links, assets and fragment targets. It refuses to replace an unmarked existing output directory. The generated tree is disposable. The Pages workflow repeats selection checks and link checks before uploading/deploying it.

Verify both wide and narrow screens, keyboard navigation, the manual/search, and a real release download flow. The Mac chip chooser must remain visible when a browser cannot reliably report the CPU. On Safari, `MacIntel` in the user agent does **not** establish Intel hardware.

## Download behavior

Download actions read `https://api.github.com/repos/pvnkmrksk/hejjetrak/releases/latest`. Only an exact known package/archive name and an HTTPS asset URL under this repository's GitHub release-download path are accepted. Release version, archive size and a SHA-256 digest are shown when available. Assets are never inferred from an unreleased source revision. A missing platform during a partial release upload produces an explicit message instead of a guessed URL. API errors and JavaScript-disabled browsers retain links to the ordinary GitHub release page.

macOS browsers without reliable architecture hints ask Apple Silicon vs Intel; Windows/Linux unsupported ARM hints do not silently select x64. Manual choices always remain available. A download displays first-launch instructions appropriate to the selected OS. macOS ad hoc signing is never described as Developer ID signing or notarization. The instructions use Apple's per-app exception, do not remove quarantine, and do not disable Gatekeeper, SmartScreen or antivirus. Linux users choose the Debian/Ubuntu `.deb` installer or portable `.tar.gz`; a missing installer asset is reported explicitly. Installing the system package may need authentication; portable extraction and ordinary launch do not. Both use distribution desktop/graphics libraries and native device permissions. Mac labels read Apple Silicon and Mac Intel while existing archive filenames remain stable.

The README's Download button links to the static site's download section, where OS selection and first-launch instructions are possible. GitHub Markdown cannot run automatic OS-detection JavaScript, so a direct GitHub Releases fallback stays next to it.

## Publish

The repository must use **Settings → Pages → Source: GitHub Actions**. `.github/workflows/pages.yml` builds on relevant main pushes and PRs; only main or manual main dispatch deploys. If Pages is not configured, it emits a notice and retains the build artifact without pretending publication succeeded.

Expected URL: `https://pvnkmrksk.github.io/hejjetrak/`, with manual under `/manual/`. If the repository or custom domain changes, update the landing page canonical/social metadata, mdBook `site-url`, README, the 404 paths and workflow URL together. External links and actual public reachability should be verified after deployment; a successful local build is not publication evidence.

## Screenshots and releases

`docs/images/preview.png` and `diagnostics.png` are actual native screenshots of the attributed sample, never private live-camera frames. Capture a fresh screenshot after UI changes, visually inspect it, and update the manual/README alt text if the view changes. Do not use mockups as runtime evidence.

The manual follows main, so installation/release chapters must distinguish published features from unreleased source changes. The latest release asset is selected dynamically, while historical benchmark numbers remain explicitly tied to their recorded fixture/build. Do not turn a single sample timing into a sixteen-camera or general energy-use claim.
