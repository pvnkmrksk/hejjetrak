# HejjéTrac v0.2.2

This release adds direct output to the original MATREX VR players, restores
Ubuntu 22.04 compatibility and fixes recovery when a tracked ball accelerates.
Native FicTrac output remains the default.

## Ubuntu installation and cameras

Linux packages are built on Ubuntu 22.04 and require glibc 2.35 or newer. Both
the `.deb` and portable archive are checked on Ubuntu 22.04 and 24.04, including
the full supplied video and a visible native preview.

Linux capture recognizes the standard `GREY` format used by monochrome cameras,
selects from the formats a camera actually advertises, and supports MJPEG with a
bounded Rust decoder. A camera that stops delivering frames returns a useful
error instead of waiting indefinitely. An occupied output port no longer blocks
the preview or recording; the port error remains visible until corrected.
The Mac capture adapter is unchanged. Physical Linux camera testing remains
outside the available validation hardware.

## Fast-motion recovery

After a sudden speed change, recovery could discard a useful motion candidate
and repeatedly restart from a stationary guess. Recovery now retains that
candidate as a search proposal and verifies the predicted pose against the
complete region of interest. Existing image-quality gates, acceleration
confirmation and evaluation limits remain in place. Missing motion is still
left unmeasured.

Known-motion tests cover sharp and temporally blurred acceleration sequences.
The full-video recovery reference retains its original accuracy limits. These
fixtures do not establish physical accuracy for arbitrary blur, exposure or
ball patterns. [Recovery details](recovery.md) and [validation](validation.md)
record the test conditions.

## Direct legacy MATREX VR output

In **Settings → Output & recording → ZeroMQ publisher**, choose
**Legacy MATREX VR (JSON)** and match the receiver's port. Use **Topic + JSON**
for paired topic/payload reads, or **Single JSON frame** for the Python
republisher's framing. Native FT is still the default, and UDP/serial retain
their FT format.

```text
zmq_host        : 127.0.0.1
zmq_port        : 9871
zmq_output_mode : legacy_matrex_vr
zmq_framing     : topic_json
zmq_topic       : FT
```

The JSON preserves the original conversion: cumulative x/y in sphere radians,
yaw in degrees, and zero z/pitch/roll. Unity applies the sphere radius. No Python
runtime or republisher is needed. The Rust publisher supports the original
NetMQ 4.0.0.1 handshake as well as modern subscribers. Only accepted, finite
measurements are published; recording remains independent of subscribers.

Unchanged MATREX VR v0.1.0 and v0.3.0 Mac binaries pass movement, heading,
reconnect and repeated scene-load checks with both framings. The old v0.1.0
receiver retains its thread/socket shutdown warnings; its clean-lifecycle check
does not pass. v0.3.0 completes the tested lifecycle cleanly. See the
[compatibility guide](matrex-vr.md) for the unchanged-player evidence and scope.

The earlier v0.2.0 and v0.2.1 candidates remain unpublished; their source tags
are retained. This release includes the deterministic Windows transport-test
fixture correction and the subsequent Ubuntu and tracking fixes.

[Downloads](https://pvnkmrksk.github.io/hejjetrak/?download=1) ·
[Installation](https://pvnkmrksk.github.io/hejjetrak/manual/install.html) ·
[User manual](https://pvnkmrksk.github.io/hejjetrak/manual/)
