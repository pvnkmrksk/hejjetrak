// Build-time checks for pure download selection; no browser or network needed.
import assert from 'node:assert/strict';
import {detectPlatform, defaultPlatform, downloadIntent, selectAsset, digestText, PLATFORMS, initializeDownloads} from '../site/assets/downloads.mjs';
assert.equal(detectPlatform({platform:'MacIntel',userAgent:'Intel Mac OS X'}),'mac-choice');
assert.equal(detectPlatform({platform:'macOS',architecture:'arm'}),'mac-arm');
assert.equal(detectPlatform({platform:'macOS',architecture:'x86'}),'mac-intel');
assert.equal(detectPlatform({platform:'Win32'}),'windows');
assert.equal(detectPlatform({platform:'Linux x86_64'}),'linux-choice');
assert.equal(detectPlatform({platform:'Linux aarch64'}),null);
assert.equal(detectPlatform({platform:'Windows',architecture:'arm'}),null);
// ARM can appear only in a platform/UA string when client hints are unavailable.
assert.equal(detectPlatform({platform:'Win32',userAgent:'Mozilla/5.0 (Windows NT 10.0; ARM64)'}),null);
assert.equal(detectPlatform({platform:'Windows ARM64'}),null);
assert.equal(detectPlatform({platform:'Windows',architecture:'aarch64'}),null);
assert.equal(detectPlatform({platform:'Windows',architecture:'ARM64EC'}),null);
assert.equal(detectPlatform({platform:'Win32',architecture:'x86',userAgent:'Windows NT 10.0; ARM64'}),null);
assert.equal(detectPlatform({platform:'Linux arm64'}),null);
assert.equal(detectPlatform({platform:'Linux armv7l'}),null);
assert.equal(detectPlatform({platform:'Linux',userAgent:'Mozilla/5.0 (X11; Linux aarch64)'}),null);
assert.equal(detectPlatform({platform:'Linux',architecture:'ARM64'}),null);
assert.equal(detectPlatform({platform:'Win32',userAgent:'Windows NT 10.0; Win64; x64'}),'windows');
assert.equal(detectPlatform({platform:'Linux',userAgent:'Mozilla/5.0 (X11; Linux x86_64)'}),'linux-choice');
assert.equal(detectPlatform({platform:'MacIntel',userAgent:'Mac OS X; ARM64'}),'mac-choice');

assert.equal(detectPlatform({platform:'MacIntel',maxTouchPoints:5}),null);
assert.equal(detectPlatform({platform:'Android',mobile:true}),null);
assert.equal(detectPlatform({}),null);
for (const [platform,spec] of Object.entries(PLATFORMS)) {
 const asset={name:spec.file,browser_download_url:`https://github.com/pvnkmrksk/hejjetrak/releases/download/v999/${spec.file}`,size:123};
 assert.equal(selectAsset({assets:[asset]},platform)?.name,spec.file);
 const legacy={...asset,name:spec.legacyFile,browser_download_url:asset.browser_download_url.replace(spec.file,spec.legacyFile)};
 assert.equal(selectAsset({assets:[legacy]},platform)?.name,spec.legacyFile);
 assert.equal(selectAsset({assets:[legacy,asset]},platform)?.name,spec.file);
 assert.equal(selectAsset({assets:[]},platform),null);
 for (const bad of ['https://evil.test/pvnkmrksk/hejjetrak/releases/download/v1/file.zip','javascript:alert(1)','https://github.com:8443/pvnkmrksk/hejjetrak/releases/download/v1/file.zip','https://github.com/another/repo/releases/download/v1/file.zip','http://github.com/pvnkmrksk/hejjetrak/releases/download/v1/file.zip']) {
  assert.equal(selectAsset({assets:[{...asset,browser_download_url:bad}]},platform),null);
 }
}
assert.equal(selectAsset(null,'linux'),null);
assert.equal(selectAsset({assets:[]},'unknown'),null);
assert.equal(digestText({digest:`sha256:${'a'.repeat(64)}`}),`sha256:${'a'.repeat(64)}`);
assert.equal(digestText({digest:'sha256:too-short'}),'');
console.log('Download platform, asset URL and digest checks passed.');

assert.equal(PLATFORMS['mac-arm'].file, 'hejjetrac-macos-arm64.zip');
assert.match(PLATFORMS['mac-arm'].label, /Apple Silicon/);
assert.match(PLATFORMS['mac-intel'].label, /Mac · Intel/);
assert.equal(selectAsset({assets:[{name:PLATFORMS.linux.file,browser_download_url:'https://github.com/pvnkmrksk/hejjetrak/releases/download/v0.1.2/'+PLATFORMS.linux.file}]},'linux-deb'),null);
assert.equal(defaultPlatform('mac-choice'),'mac-arm');
assert.equal(defaultPlatform('linux-choice'),'linux');
assert.equal(defaultPlatform('mac-intel'),'mac-intel');
assert.equal(defaultPlatform(null),null);
assert.equal(downloadIntent('https://example.com/?download=1'),true);
for (const url of ['https://example.com/','https://example.com/?download=0','https://example.com/#download=1','broken']) assert.equal(downloadIntent(url),false);

// Exercise the actual dialog controller without a DOM library or network.
function fakeNode(dataset = {}) {
 return {dataset, hidden:true, textContent:'', href:'', open:false, clicks:0,
   addEventListener(event, handler) {this[event] = handler;},
   showModal() {this.open=true;}, click() {this.clicks++;}};
}
const nodes = Object.fromEntries(['download-dialog','download-title','download-description',
 'chip-choice','package-choice','download-details','asset-download','asset-meta',
 'asset-digest','download-status','release-version','download-assumption','download-version-note','instructions-mac',
 'instructions-windows','instructions-linux','instructions-linux-deb'].map(id=>[id,fakeNode()]));
const buttons = ['linux-choice','linux','linux-deb','mac-arm','windows'].map(platform=>fakeNode({platform}));
const launchers = ['mac','windows'].map(launcher=>fakeNode({launcher}));
const fakeDocument = {getElementById:id=>nodes[id], querySelectorAll:selector=>
 selector==='[data-platform]'?buttons:selector==='[data-launcher]'?launchers:[]};
const originalFetch=globalThis.fetch;
const assetFor=(platform,tag='v0.1.4')=>({name:PLATFORMS[platform].file,
 browser_download_url:`https://github.com/pvnkmrksk/hejjetrak/releases/download/${tag}/${PLATFORMS[platform].file}`});
const responseFor=(assets)=>({ok:true,json:async()=>({tag_name:'v0.1.4',assets})});
try {
 globalThis.fetch=async()=>({ok:true,json:async()=>({tag_name:'v0.1.3',assets:[{
   name:PLATFORMS['linux-deb'].file,
   browser_download_url:'https://github.com/pvnkmrksk/hejjetrak/releases/download/v0.1.3/'+PLATFORMS['linux-deb'].file,
 }]})});
 initializeDownloads(fakeDocument);
 await buttons[0].click();
 assert.equal(nodes['package-choice'].hidden,false);
 assert.equal(nodes['download-details'].hidden,true);
 await buttons[2].click();
 assert.equal(nodes['package-choice'].hidden,true);
 assert.equal(nodes['instructions-linux-deb'].hidden,false);
 assert.equal(nodes['instructions-linux'].hidden,true);
 assert.equal(nodes['download-details'].hidden,false);
 assert.equal(nodes['asset-download'].clicks,1);
 assert.match(nodes['asset-download'].href,/hejjetrac-linux-amd64\.deb$/);
 await buttons[1].click(); // Missing portable asset must not trigger a second download.
 assert.equal(nodes['instructions-linux-deb'].hidden,true);
 assert.equal(nodes['instructions-linux'].hidden,false);
 assert.equal(nodes['download-details'].hidden,true);
 assert.equal(nodes['asset-download'].clicks,1);
 assert.match(nodes['download-description'].textContent,/not available/);
} finally {globalThis.fetch=originalFetch;}
console.log('Linux format chooser, installer URL and missing-asset dialog checks passed.');

// A legacy fallback needs matching opening instructions. Switching to a current
// package clears that note and restores both modern launcher names.
for (const [legacyPlatform,currentPlatform,launcherKind,legacyName] of [
 ['mac-arm','windows','mac','hejjetrak.app'],
 ['windows','mac-arm','windows','hejjetrak.exe'],
]) {
 try {
  const spec=PLATFORMS[legacyPlatform];
  const legacy={name:spec.legacyFile,
   browser_download_url:`https://github.com/pvnkmrksk/hejjetrak/releases/download/v0.1.3/${spec.legacyFile}`};
  globalThis.fetch=async()=>({ok:true,json:async()=>({tag_name:'v0.1.3',assets:[legacy,assetFor(currentPlatform)]})});
  const before=nodes['asset-download'].clicks;
  await initializeDownloads(fakeDocument,{pageUrl:'https://example.com/?download=1',
   resolvePlatform:async()=>legacyPlatform,consumeIntent:()=>{}}).automatic;
  assert.equal(nodes['asset-download'].href,legacy.browser_download_url);
  assert.equal(nodes['asset-download'].clicks,before+1);
  assert.equal(nodes['download-version-note'].hidden,false);
  assert.match(nodes['download-version-note'].textContent,/v0\.1\.3.*earlier hejjetrak name and calibration interface/);
  assert.equal(launchers.find(node=>node.dataset.launcher===launcherKind).textContent,legacyName);
  await buttons.find(node=>node.dataset.platform===currentPlatform).click();
  assert.equal(nodes['download-version-note'].hidden,true);
  assert.equal(nodes['asset-download'].href,assetFor(currentPlatform).browser_download_url);
  assert.equal(nodes['asset-download'].clicks,before+2);
  assert.deepEqual(launchers.map(node=>node.textContent),['hejjetrac.app','hejjetrac.exe']);
 } finally {globalThis.fetch=originalFetch;}
}
console.log('Legacy launcher names and calibration note clear when switching to current packages.');

// Closing an in-flight chooser must not start a late attachment download.
let completeRelease;
try {
 globalThis.fetch = () => new Promise(resolve => { completeRelease = resolve; });
 initializeDownloads(fakeDocument);
 const pending = buttons[2].click();
 nodes['download-dialog'].open = false;
 completeRelease({ok:true,json:async()=>({tag_name:'v1',assets:[{
  name:PLATFORMS['linux-deb'].file,
  browser_download_url:'https://github.com/pvnkmrksk/hejjetrak/releases/download/v1/'+PLATFORMS['linux-deb'].file,
 }]})});
 const previous = nodes['asset-download'].clicks;
 await pending;
 nodes['download-dialog'].close(); // Native close event can follow the metadata promise.
 assert.equal(nodes['asset-download'].clicks,previous);
 assert.equal(nodes['download-details'].hidden,true);
} finally {globalThis.fetch=originalFetch;}
console.log('Closing the chooser cancels pending automatic downloads.');

// A README deep link starts exactly one verified attachment download. Ambiguous
// Mac hardware gets the stated default, with manual alternatives still bound.
for (const [detected,expected] of [['mac-choice','mac-arm'],['mac-intel','mac-intel'],['windows','windows'],['linux-choice','linux']]) {
 let consumed=0,fetches=0;
 try {
  globalThis.fetch=async()=>{fetches++;return responseFor([assetFor(expected)]);};
  const before=nodes['asset-download'].clicks;
  const controller=initializeDownloads(fakeDocument,{pageUrl:'https://example.com/?download=1',resolvePlatform:async()=>detected,consumeIntent:()=>{consumed++;}});
  await controller.automatic;
  assert.equal(consumed,1);
  assert.equal(fetches,1);
  assert.equal(nodes['asset-download'].clicks,before+1);
  assert.equal(nodes['asset-download'].href,assetFor(expected).browser_download_url);
  assert.equal(nodes['download-assumption'].hidden,detected!=='mac-choice');
 } finally {globalThis.fetch=originalFetch;}
}
try {
 let fetches=0;
 globalThis.fetch=async()=>{fetches++;return responseFor([]);};
 const before=nodes['asset-download'].clicks;
 await initializeDownloads(fakeDocument,{pageUrl:'https://example.com/?download=1',resolvePlatform:async()=>null,consumeIntent:()=>{}}).automatic;
 assert.equal(fetches,0);
 assert.equal(nodes['asset-download'].clicks,before);
 assert.match(nodes['download-description'].textContent,/could not be matched/);
 await initializeDownloads(fakeDocument,{pageUrl:'https://example.com/',resolvePlatform:async()=>{throw Error('Not requested');}}).automatic;
 assert.equal(fetches,0);
} finally {globalThis.fetch=originalFetch;}

// Closing while client hints are pending must also cancel, before any fetch.
try {
 let resolvePlatform,fetches=0;
 globalThis.fetch=async()=>{fetches++;return responseFor([assetFor('mac-arm')]);};
 const before=nodes['asset-download'].clicks;
 const controller=initializeDownloads(fakeDocument,{pageUrl:'https://example.com/?download=1',resolvePlatform:()=>new Promise(resolve=>{resolvePlatform=resolve;}),consumeIntent:()=>{}});
 nodes['download-dialog'].open=false;nodes['download-dialog'].close();
 resolvePlatform('mac-choice');await controller.automatic;
 assert.equal(fetches,0);assert.equal(nodes['asset-download'].clicks,before);
} finally {globalThis.fetch=originalFetch;}
console.log('README automatic downloads, defaults, intent consumption and pending-detection cancellation passed.');
