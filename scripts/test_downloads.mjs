// Build-time checks for pure download selection; no browser or network needed.
import assert from 'node:assert/strict';
import {detectPlatform, selectAsset, digestText, PLATFORMS} from '../site/assets/downloads.mjs';
assert.equal(detectPlatform({platform:'MacIntel',userAgent:'Intel Mac OS X'}),'mac-choice');
assert.equal(detectPlatform({platform:'macOS',architecture:'arm'}),'mac-arm');
assert.equal(detectPlatform({platform:'macOS',architecture:'x86'}),'mac-intel');
assert.equal(detectPlatform({platform:'Win32'}),'windows');
assert.equal(detectPlatform({platform:'Linux x86_64'}),'linux');
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
assert.equal(detectPlatform({platform:'Linux',userAgent:'Mozilla/5.0 (X11; Linux x86_64)'}),'linux');
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
