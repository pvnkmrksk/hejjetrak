// Build-time checks for pure download selection; no browser or network needed.
import assert from 'node:assert/strict';
import {detectPlatform, selectAsset, digestText, PLATFORMS, initializeDownloads} from '../site/assets/downloads.mjs';
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

assert.equal(PLATFORMS['mac-arm'].file, 'hejjetrak-macos-arm64.zip');
assert.match(PLATFORMS['mac-arm'].label, /Apple Silicon/);
assert.match(PLATFORMS['mac-intel'].label, /Mac Intel/);
assert.equal(selectAsset({assets:[{name:PLATFORMS.linux.file,browser_download_url:'https://github.com/pvnkmrksk/hejjetrak/releases/download/v0.1.2/'+PLATFORMS.linux.file}]},'linux-deb'),null);

// Exercise the actual dialog controller without a DOM library or network.
function fakeNode(dataset = {}) {
 return {dataset, hidden:true, textContent:'', href:'', open:false, clicks:0,
   addEventListener(event, handler) {this[event] = handler;},
   showModal() {this.open=true;}, click() {this.clicks++;}};
}
const nodes = Object.fromEntries(['download-dialog','download-title','download-description',
 'chip-choice','package-choice','download-details','asset-download','asset-meta',
 'asset-digest','download-status','release-version','instructions-mac',
 'instructions-windows','instructions-linux','instructions-linux-deb'].map(id=>[id,fakeNode()]));
const buttons = ['linux-choice','linux','linux-deb'].map(platform=>fakeNode({platform}));
const fakeDocument = {getElementById:id=>nodes[id], querySelectorAll:selector=>selector==='[data-platform]'?buttons:[]};
const originalFetch=globalThis.fetch;
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
 assert.match(nodes['asset-download'].href,/hejjetrak-linux-amd64\.deb$/);
 await buttons[1].click(); // Missing portable asset must not trigger a second download.
 assert.equal(nodes['instructions-linux-deb'].hidden,true);
 assert.equal(nodes['instructions-linux'].hidden,false);
 assert.equal(nodes['download-details'].hidden,true);
 assert.equal(nodes['asset-download'].clicks,1);
 assert.match(nodes['download-description'].textContent,/does not currently include/);
} finally {globalThis.fetch=originalFetch;}
console.log('Linux format chooser, installer URL and missing-asset dialog checks passed.');
