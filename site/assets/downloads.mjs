// Static-site helper only. The native application never loads this code.
export const RELEASES = 'https://github.com/pvnkmrksk/hejjetrak/releases/latest';
export const RELEASE_API = 'https://api.github.com/repos/pvnkmrksk/hejjetrak/releases/latest';
export const PLATFORMS = Object.freeze({
  'mac-arm': {label: 'macOS · Apple Silicon', file: 'hejjetrak-macos-arm64.zip', instructions: 'mac'},
  'mac-intel': {label: 'macOS · Intel', file: 'hejjetrak-macos-x86_64.zip', instructions: 'mac'},
  windows: {label: 'Windows · x64', file: 'hejjetrak-windows-amd64.zip', instructions: 'windows'},
  linux: {label: 'Linux · x86_64', file: 'hejjetrak-linux-x86_64.tar.gz', instructions: 'linux'},
});

export function detectPlatform({platform = '', userAgent = '', architecture = '', mobile = false, maxTouchPoints = 0} = {}) {
  const text = `${platform} ${userAgent}`;
  if (mobile || /Android|iPhone|iPad|iPod/i.test(text) || (/Mac/i.test(text) && maxTouchPoints > 1)) return null;
  if (/Mac/i.test(text)) {
    // Safari can say Intel on an M-series Mac. Do not infer its chip from UA.
    if (/^(arm|arm64|aarch64)$/i.test(architecture)) return 'mac-arm';
    if (/^(x86|x86_64|amd64)$/i.test(architecture)) return 'mac-intel';
    return 'mac-choice';
  }
  // Explicit ARM hints from any exposed source must not select an x64 archive.
  const armHint = /\b(?:arm(?:64(?:ec)?|v\d+[a-z]*)?|aarch64)\b/i.test(`${architecture} ${text}`);
  if (/Win/i.test(text)) return armHint ? null : 'windows';
  if (/Linux/i.test(text)) return armHint ? null : 'linux';
  return null;
}

export function selectAsset(release, platform) {
  const spec = PLATFORMS[platform];
  if (!spec || !release || !Array.isArray(release.assets)) return null;
  const asset = release.assets.find(value => value && value.name === spec.file);
  if (!asset || typeof asset.browser_download_url !== 'string') return null;
  let url;
  try { url = new URL(asset.browser_download_url); } catch { return null; }
  if (url.origin !== 'https://github.com' || url.username || url.password ||
      !url.pathname.startsWith('/pvnkmrksk/hejjetrak/releases/download/')) return null;
  return {...asset, browser_download_url: url.href};
}

export function digestText(asset) {
  return typeof asset?.digest === 'string' && /^sha256:[a-f0-9]{64}$/i.test(asset.digest) ? asset.digest : '';
}

async function browserPlatform() {
  let architecture = '';
  const data = navigator.userAgentData;
  if (data?.getHighEntropyValues) {
    try { architecture = (await data.getHighEntropyValues(['architecture'])).architecture ?? ''; } catch { /* Manual chip choice remains available. */ }
  }
  return detectPlatform({platform: data?.platform ?? navigator.platform, userAgent: navigator.userAgent,
    architecture, mobile: data?.mobile ?? false, maxTouchPoints: navigator.maxTouchPoints});
}

export function initializeDownloads(doc = document) {
  const dialog = doc.getElementById('download-dialog');
  if (!dialog || typeof dialog.showModal !== 'function') return;
  const title = doc.getElementById('download-title');
  const description = doc.getElementById('download-description');
  const choice = doc.getElementById('chip-choice');
  const details = doc.getElementById('download-details');
  const link = doc.getElementById('asset-download');
  const meta = doc.getElementById('asset-meta');
  const digest = doc.getElementById('asset-digest');
  const status = doc.getElementById('download-status');
  const version = doc.getElementById('release-version');
  let releasePromise;
  let requestSerial = 0;

  function show() { if (!dialog.open) dialog.showModal(); }
  function clear() {
    choice.hidden = true;
    details.hidden = true;
    for (const kind of ['mac', 'windows', 'linux']) doc.getElementById(`instructions-${kind}`).hidden = true;
    digest.hidden = true;
    link.href = RELEASES;
  }
  async function release() {
    if (!releasePromise) {
      releasePromise = (async () => {
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 8000);
        try {
          const response = await fetch(RELEASE_API, {headers: {Accept: 'application/vnd.github+json'}, signal: controller.signal});
          if (!response.ok) throw new Error(`GitHub returned ${response.status}`);
          const value = await response.json();
          if (!Array.isArray(value.assets) || typeof value.tag_name !== 'string') throw new Error('Unexpected release metadata');
          return value;
        } finally { clearTimeout(timeout); }
      })().catch(error => { releasePromise = undefined; throw error; });
    }
    return releasePromise;
  }
  async function choose(platform) {
    const serial = ++requestSerial;
    clear(); show();
    if (platform === 'mac-choice') {
      title.textContent = 'Which Mac do you use?';
      description.textContent = 'Choose the matching archive; both contain the same application.';
      choice.hidden = false;
      return;
    }
    const spec = PLATFORMS[platform];
    if (!spec) {
      title.textContent = 'Choose your computer';
      description.textContent = 'Automatic detection is unavailable for this device. Close this panel and choose a supported platform below, or use the GitHub release page.';
      return;
    }
    title.textContent = `Download for ${spec.label}`;
    description.textContent = 'Checking the latest published GitHub release…';
    doc.getElementById(`instructions-${spec.instructions}`).hidden = false;
    try {
      const value = await release();
      if (serial !== requestSerial) return;
      version.textContent = `Latest published release: ${value.tag_name}`;
      const asset = selectAsset(value, platform);
      if (!asset) {
        description.textContent = `The latest release (${value.tag_name}) does not currently include this archive. It may still be building. Check all assets on GitHub or try again later.`;
        status.textContent = 'This platform archive is not present in the latest release.';
        return;
      }
      link.href = asset.browser_download_url;
      link.textContent = 'Download again';
      meta.textContent = `${value.tag_name} · ${asset.name}${Number.isFinite(asset.size) ? ` · ${(asset.size / 1_000_000).toFixed(1)} MB` : ''}`;
      const hash = digestText(asset);
      if (hash) { digest.textContent = hash; digest.hidden = false; }
      description.textContent = 'Your archive download has been requested. If the browser does not start it, use the button below. Read these first-launch steps while it downloads.';
      details.hidden = false;
      status.textContent = `Selected ${spec.label}, ${value.tag_name}.`;
      // GitHub serves the asset as an attachment; no pop-up or guessed URL is used.
      link.click();
    } catch {
      if (serial !== requestSerial) return;
      description.textContent = 'GitHub release metadata could not be loaded. Use the latest release link below to choose the platform archive directly. No download URL has been guessed.';
      status.textContent = 'Automatic download unavailable. All archives remain accessible on GitHub.';
    }
  }
  for (const button of doc.querySelectorAll('[data-platform]')) button.addEventListener('click', () => choose(button.dataset.platform));
  for (const button of doc.querySelectorAll('[data-download-auto]')) button.addEventListener('click', async event => {
    event.preventDefault();
    clear(); show();
    title.textContent = 'Finding your download';
    description.textContent = 'Detecting the operating system. Manual choices are always available below.';
    await choose(await browserPlatform());
  });
}

if (typeof document !== 'undefined') initializeDownloads();
