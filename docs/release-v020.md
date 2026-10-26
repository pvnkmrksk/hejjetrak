# HejjéTrac v0.2.0

HejjéTrac can now send pose data directly to the original MATREX VR v0.1.0 and
v0.3.0 players. The new **Legacy MATREX VR (JSON)** mode replaces the former
Python republisher. **Native FicTrac (FT) remains the default**, including for
existing configuration files.

## Connect a legacy MATREX rig

Open **Settings → Output & recording → ZeroMQ publisher**, enable the publisher,
choose **Legacy MATREX VR (JSON)** and match the receiver's port. Select
**Topic + JSON** for v0.1.0's paired reads; v0.3.0 accepts either that framing or
**Single JSON frame**. Save and apply the configuration before the experiment.

```text
zmq_host        : 127.0.0.1
zmq_port        : 9871
zmq_output_mode : legacy_matrex_vr
zmq_framing     : topic_json
zmq_topic       : FT
```

HejjéTrac binds the localhost port; Unity connects. Give each rig a different
port. `single` sends one JSON frame; `topic_json` sends `[topic, JSON]` as one
multipart message. The topic defaults to `FT`. Set the port to `-1` or `0` to
disable the publisher, or set the output mode to `native_ft` to use ordinary FT
rows. UDP and serial continue to use their FT format in either mode.

The legacy JSON preserves the original Python conversion: cumulative x/y in
sphere radians, yaw in degrees, and zero z/pitch/roll. Unity applies the sphere
radius. Key order, decimal conversion and payload formatting match the former
republisher; HejjéTrac adds no axis swap, origin offset or radius scaling.

## Rust transport, independent recording

The publisher now negotiates ZMTP 2.0 with the original **NetMQ 4.0.0.1** DLL and
NULL-authenticated ZMTP 3 with modern subscribers. Serialization and transport
run entirely in Rust, without Python, .NET or a native libzmq runtime.

Only accepted, finite measurements are published. Subscriber connections,
disconnects and stalled handshakes do not control recording or numerical
processing. Recordings retain all-frame numeric and quality evidence, while
each subscriber has its own bounded queue and write deadline. Live PUB/SUB
output remains best effort, with no replay for a subscriber joining late.

## Compatibility checks

The unchanged Mac players passed the functional matrix with **four rigs and
six complete scene loads per case**, including heading wrap, rejected marker
rows and a publisher disconnect/rebind. Their executables and managed
assemblies, including the original NetMQ DLL, were hash-checked unchanged.

| Original player | Single JSON | Topic + JSON | Receiver shutdown |
|---|---|---|---|
| MATREX VR v0.1.0 | Pass | Pass | Existing lifecycle issue remains |
| MATREX VR v0.3.0 | Pass | Pass | Clean in all tested cases |

The exact old DLL also passed byte-for-byte payload and reconnect checks in
both framings. An independent modern libzmq subscriber received **900 real
sample rows** exactly and in order: 300 native FT, 300 single JSON and 300
multipart JSON. Native FT and UDP bytes remained unchanged. Real-video
recording checks retained all 300 numeric/quality rows with no subscriber and
with an incomplete TCP greeting.

The old v0.1.0 receiver still produced **nine and twelve worker-stop timeout
warnings** in the single/multipart cases. Its recording shutdown can also
report an archive-sharing error. Functional movement, heading, reconnect and
scene reloads pass, but its **strict clean-lifecycle result remains false**.
This release does not repair the original receiver's thread/socket teardown.
The v0.3.0 receiver completed the tested lifecycle checks cleanly.

These legacy-player checks ran headless on Mac and verified recorded Unity
transforms. They do not cover Windows/Linux legacy players, physical camera
accuracy or unlimited scene reloads.

[MATREX VR setup and complete evidence](matrex-vr.md) ·
[Online manual](https://pvnkmrksk.github.io/hejjetrak/manual/matrex-vr.html) ·
[Output reference](output.md) · [Validation](validation.md)
