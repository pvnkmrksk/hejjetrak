// Static website only; the native application never loads this code.
export const RELEASES = 'https://github.com/pvnkmrksk/hejjetrak/releases/latest';
export const RELEASE_API = 'https://api.github.com/repos/pvnkmrksk/hejjetrak/releases/latest';
export const PLATFORMS = Object.freeze({
  'mac-arm': {label:'Mac · Apple Silicon',file:'hejjetrac-macos-arm64.zip',legacyFile:'hejjetrak-macos-arm64.zip',instructions:'mac'},
  'mac-intel': {label:'Mac · Intel',file:'hejjetrac-macos-x86_64.zip',legacyFile:'hejjetrak-macos-x86_64.zip',instructions:'mac'},
  windows: {label:'Windows · x64',file:'hejjetrac-windows-amd64.zip',legacyFile:'hejjetrak-windows-amd64.zip',instructions:'windows'},
  linux: {label:'Linux · portable x86_64',file:'hejjetrac-linux-x86_64.tar.gz',legacyFile:'hejjetrak-linux-x86_64.tar.gz',instructions:'linux'},
  'linux-deb': {label:'Debian / Ubuntu · amd64',file:'hejjetrac-linux-amd64.deb',legacyFile:'hejjetrak-linux-amd64.deb',instructions:'linux-deb'},
});
export function detectPlatform({platform='',userAgent='',architecture='',mobile=false,maxTouchPoints=0}={}) {
  const text=`${platform} ${userAgent}`;
  if (mobile || /Android|iPhone|iPad|iPod/i.test(text) || (/Mac/i.test(text) && maxTouchPoints>1)) return null;
  if (/Mac/i.test(text)) {
    // Safari's "MacIntel" also appears on Apple Silicon; it is not a chip report.
    if (/^(arm|arm64|aarch64)$/i.test(architecture)) return 'mac-arm';
    if (/^(x86|x86_64|amd64)$/i.test(architecture)) return 'mac-intel';
    return 'mac-choice';
  }
  const armHint=/\b(?:arm(?:64(?:ec)?|v\d+[a-z]*)?|aarch64)\b/i.test(`${architecture} ${text}`);
  if (/Win/i.test(text)) return armHint?null:'windows';
  if (/Linux/i.test(text)) return armHint?null:'linux-choice';
  return null;
}
export function defaultPlatform(detected) {
  if (detected==='mac-choice') return 'mac-arm';
  if (detected==='linux-choice') return 'linux';
  return detected;
}
export function downloadIntent(url) {
  try {return new URL(url).searchParams.get('download')==='1';} catch {return false;}
}
export function selectAsset(release,platform) {
  const spec=PLATFORMS[platform];
  if (!spec || !Array.isArray(release?.assets)) return null;
  const asset=[spec.file,spec.legacyFile].map(name=>release.assets.find(value=>value?.name===name)).find(Boolean);
  if (typeof asset?.browser_download_url!=='string') return null;
  let url;
  try {url=new URL(asset.browser_download_url);} catch {return null;}
  if (url.origin!=='https://github.com' || url.username || url.password || !url.pathname.startsWith('/pvnkmrksk/hejjetrak/releases/download/')) return null;
  return {...asset,browser_download_url:url.href};
}
export function digestText(asset) {
  return typeof asset?.digest==='string' && /^sha256:[a-f0-9]{64}$/i.test(asset.digest)?asset.digest:'';
}
async function browserPlatform() {
  let architecture='';
  const data=navigator.userAgentData;
  if (data?.getHighEntropyValues) {
    try {architecture=(await data.getHighEntropyValues(['architecture'])).architecture??'';} catch { /* The recommended default and manual choices remain available. */ }
  }
  return detectPlatform({platform:data?.platform??navigator.platform,userAgent:navigator.userAgent,
    architecture,mobile:data?.mobile??false,maxTouchPoints:navigator.maxTouchPoints});
}
export function initializeDownloads(doc=document,{resolvePlatform=browserPlatform,pageUrl=globalThis.location?.href,consumeIntent}={}) {
  const dialog=doc.getElementById('download-dialog');
  if (!dialog || typeof dialog.showModal!=='function') return;
  const node=id=>doc.getElementById(id);
  const title=node('download-title'),description=node('download-description'),choice=node('chip-choice'),packages=node('package-choice');
  const details=node('download-details'),link=node('asset-download'),meta=node('asset-meta'),digest=node('asset-digest');
  const status=node('download-status'),version=node('release-version'),assumption=node('download-assumption'),legacyNote=node('download-version-note');
  let releasePromise,requestSerial=0;
  dialog.addEventListener('close',()=>{requestSerial++;});
  function clear() {
    choice.hidden=packages.hidden=details.hidden=digest.hidden=true;
    if (assumption) assumption.hidden=true;
    if (legacyNote) legacyNote.hidden=true;
    for (const launcher of doc.querySelectorAll('[data-launcher]')) launcher.textContent=launcher.dataset.launcher==='mac'?'hejjetrac.app':'hejjetrac.exe';
    for (const kind of ['mac','windows','linux','linux-deb']) node(`instructions-${kind}`).hidden=true;
    link.href=RELEASES;
  }
  function show() {if (!dialog.open) dialog.showModal();}
  async function release() {
    if (!releasePromise) {
      releasePromise=(async()=>{
        const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),8000);
        try {
          const response=await fetch(RELEASE_API,{headers:{Accept:'application/vnd.github+json'},cache:'no-store',signal:controller.signal});
          if (!response.ok) throw new Error('Release metadata unavailable');
          const value=await response.json();
          if (!Array.isArray(value.assets) || typeof value.tag_name!=='string') throw new Error('Invalid release metadata');
          return value;
        } finally {clearTimeout(timeout);}
      })().catch(error=>{releasePromise=undefined;throw error;});
    }
    return releasePromise;
  }
  async function choose(platform,{assumedMac=false}={}) {
    const serial=++requestSerial;
    clear();show();
    if (platform==='mac-choice') {
      title.textContent='Choose your Mac';description.textContent='Apple Silicon or Intel: check Apple menu → About This Mac.';choice.hidden=false;return;
    }
    if (platform==='linux-choice') {
      title.textContent='Choose your Linux download';description.textContent='Use the portable archive, or the Debian/Ubuntu installer.';packages.hidden=false;return;
    }
    const spec=PLATFORMS[platform];
    if (!spec) {
      title.textContent='Choose a desktop download';description.textContent='This device could not be matched to a supported build. Choose another platform below.';return;
    }
    if (assumedMac && assumption) {
      assumption.hidden=false;
      assumption.textContent='Your browser does not report the Mac chip. Apple Silicon is the default; choose Mac Intel below if your Mac has an Intel processor.';
    }
    title.textContent=`Download for ${spec.label}`;
    description.textContent='Checking the latest release…';
    node(`instructions-${spec.instructions}`).hidden=false;
    try {
      const value=await release();
      if (serial!==requestSerial || !dialog.open) return;
      version.textContent=`Latest release: ${value.tag_name}`;
      const asset=selectAsset(value,platform);
      if (!asset) {
        description.textContent=`This archive is not available in ${value.tag_name}. See all release downloads below.`;
        status.textContent='The selected archive is not available.';return;
      }
      link.href=asset.browser_download_url;link.textContent='Download again';
      if (asset.name===spec.legacyFile) {
        for (const launcher of doc.querySelectorAll('[data-launcher]')) launcher.textContent=launcher.dataset.launcher==='mac'?'hejjetrak.app':'hejjetrak.exe';
        if (legacyNote) {
          legacyNote.textContent=`${value.tag_name} uses the earlier hejjetrak name and calibration interface. The redesigned calibration is available from v0.1.4.`;
          legacyNote.hidden=false;
        }
      }
      meta.textContent=`${value.tag_name} · ${asset.name}${Number.isFinite(asset.size)?` · ${(asset.size/1_000_000).toFixed(1)} MB`:''}`;
      const hash=digestText(asset);
      if (hash) {digest.textContent=hash;digest.hidden=false;}
      description.textContent='Your download is starting. The button below is available if your browser needs another click.';
      details.hidden=false;status.textContent=`Selected ${spec.label}, ${value.tag_name}.`;
      link.click();
    } catch {
      if (serial!==requestSerial || !dialog.open) return;
      description.textContent='The release service could not be reached. Choose your archive on GitHub using the release link below.';
      status.textContent='Automatic download unavailable; use GitHub releases.';
    }
  }
  async function automaticDownload() {
    const serial=++requestSerial;
    clear();show();title.textContent='Finding your download';description.textContent='Detecting your computer…';
    const detected=await resolvePlatform();
    if (serial!==requestSerial || !dialog.open) return;
    await choose(defaultPlatform(detected),{assumedMac:detected==='mac-choice'});
  }
  for (const button of doc.querySelectorAll('[data-platform]')) button.addEventListener('click',()=>choose(button.dataset.platform));
  for (const button of doc.querySelectorAll('[data-download-auto]')) button.addEventListener('click',event=>{event.preventDefault();return automaticDownload();});
  const automatic=downloadIntent(pageUrl);
  if (automatic) {
    if (consumeIntent) consumeIntent();
    else if (globalThis.history && globalThis.location) {
      const clean=new URL(pageUrl);clean.searchParams.delete('download');
      globalThis.history.replaceState(globalThis.history.state,'',clean.href);
    }
  }
  return {automatic:automatic?automaticDownload():Promise.resolve()};
}
if (typeof document!=='undefined') initializeDownloads();
