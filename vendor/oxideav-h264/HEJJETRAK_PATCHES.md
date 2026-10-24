# Local performance patch

Source: the MIT-licensed `oxideav-h264` 0.1.9 crates.io distribution, package SHA256
`6757ee6e7db4622536409825bec9fda8642e8225f8e49fb9331d995e9e357322`.
Upstream: <https://github.com/OxideAV/oxideav-h264>. Upstream source, tests,
authorship and license are retained. This is an excluded third-party package,
not a second application engine. No C/C++ dependency or nightly feature is added.

Changes are restricted to:

- `cabac_ctx.rs`: lazy diagnostic formatting, avoiding coefficient-by-coefficient
  string allocation when diagnostics are off.
- `cabac.rs`: cache bin tracing per decoder and inline small entropy operations;
  decoded state transitions and bin accounting are unchanged.
- `bitstream.rs`: consume multi-bit fields by byte fragments, and locate the RBSP
  stop bit from the tail instead of rescanning every byte/bit for each macroblock.
- `slice_data.rs`: read macroblock trace switches once per slice rather than
  calling the process environment for each macroblock.

Full-video luma/RGB hashes, presentation order, existing codec fixtures, and
upstream entropy/bitstream tests must pass without changing expected constants.
`scripts/package.py` includes this package's MIT license in binary archives;
Cargo's registry vendor command alone does not collect local path dependencies.
