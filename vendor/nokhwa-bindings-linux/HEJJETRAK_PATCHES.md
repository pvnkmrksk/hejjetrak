# Linux camera adapter patches

Vendored from `nokhwa-bindings-linux` 0.1.4, crates.io SHA-256
`bb67e22201a53322291740ca064b20eaaade7222ef0349f312d9b37b004e1984`.
Original copyright and Apache-2.0 terms are retained.

- Accept standard V4L2 `GREY` and legacy `GRAY` mono identifiers, emitting
  `GREY` when selecting mono capture. Mono UVC cameras otherwise disappear
  from the set of usable formats.
- Bound an MMAP frame read to two seconds. Failure is propagated; the app
  closes the stream and offers reconnect/fallback. It must not retry the same
  timed-out `v4l` MMAP queue, whose pending buffer state is not retry-safe.
- Advertise only single-plane streaming image-capture nodes, excluding UVC
  metadata companions, and sort by actual device index.
- Preserve sparse V4L2 menu entries as typed enums, including exposure modes.
  Accept enum setters and verify their native readback. A menu containing only
  manual `1` and aperture-priority `3` must not fabricate support for entry `0`.

Application JPEG decoding remains pure Rust, outside Nokhwa's optional
`mozjpeg` feature. No C/C++ media runtime or subprocess is added.

Linux-only regression:
`cargo test -p nokhwa-bindings-linux --lib --locked`
