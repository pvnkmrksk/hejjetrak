# Legacy MATREX VR compatibility

HejjéTrac v0.2.0 adds an explicit output mode for the unchanged MATREX VR
v0.1.0 and v0.3.0 receivers. **Native FT remains the default.** The old v0.1.4
downloads predate this mode.

The application uses Rust for serialization and transport. It needs no Python
republisher, Python runtime, .NET installation or native libzmq library.
The Python and C# files under `scripts/` are optional interoperability test
tools; they are not application components.

## Set up a rig

The saved repository rigs enable UDP ports 1111–1114 and direct ZMQ ports
9871–9874. When using the existing four Python republishers, those programs
own ports 9871–9874, so HejjéTrac's direct publishers must be disabled for that
launch. From this repository, start all four calibrated rigs through the bridge
with:

```sh
./scripts/run_four_bridged.sh
```

The launcher uses the paired Rust CLI to copy `VR1.txt`–`VR4.txt` into one
temporary folder with only `zmq_port : 0` changed. Camera IDs, UDP destinations,
calibration and the original files stay intact. It opens those copies in a 2×2
grid and retains them until the app exits, then removes the temporary folder.
To keep calibration edits made during this bridged session, use **Save as** to
a permanent rig file before quitting. Video recording stays off until selected
in the UI.

The launcher looks for the extracted Linux portable app under
`dist/hejjetrac-linux-x86_64`, then the local release build. You can choose
executables with `HEJJETRAC_BINARY` and `HEJJETRAC_CLI`; otherwise it finds the
CLI beside the app or at `target/release/fictrac`. Keep the four Python
republishers running. Close original FicTrac and any other camera-owning tracker
windows before taking over all four cameras; the launcher does not terminate
other programs or steal their devices.

## Alternative: direct publisher without Python

Use this alternative only on a port that the Python republishers do not own.

Open **Settings → Output & recording → ZeroMQ publisher**. Choose
**Legacy MATREX VR (JSON)** and a port matching the VR receiver. Use
**Topic + JSON** for v0.1.0's paired topic/payload reads. v0.3.0 accepts either
framing; **Single JSON frame** also matches the former Python republisher.
Save the rig file, and apply the settings before acquiring an experiment.
Normal configuration/epoch rules still apply to edits; a subscriber joining or
disconnecting does not change the tracker or close its recording.

Equivalent configuration:

```text
zmq_host           : 127.0.0.1
zmq_port           : 9871
zmq_output_mode    : legacy_matrex_vr
zmq_framing        : topic_json
zmq_topic          : FT
zmq_queue_capacity : 256
```

The publisher binds; Unity connects. Use a unique, unused port for each rig.
The default bind address remains localhost. Set `zmq_port` to `-1` or `0` to disable the
publisher. To retain native output, omit the new fields or set
`zmq_output_mode : native_ft`; native FT always sends a single FT frame,
regardless of a stored JSON framing preference. UDP and serial retain FT rows
in both modes.

### A VR receiver on another computer

Set `zmq_host` to an interface address **on the tracking computer**, or explicitly
use `0.0.0.0` / `*` to bind all its IPv4 interfaces. Numeric IPv6 interface
addresses and `::` for IPv6 interfaces are also supported. Only `localhost` and
numeric addresses are accepted; the publisher does not perform DNS resolution.
Do not enter the remote VR computer's IP as the bind address.

For example, if the tracker is `10.126.17.125` and a VR2 receiver is on
`10.126.17.75`, the tracker can use:

```text
zmq_host           : 0.0.0.0
zmq_port           : 9872
zmq_output_mode    : legacy_matrex_vr
zmq_framing        : single
```

The VR receiver connects to `tcp://10.126.17.125:9872`. Use `topic_json` instead
when that receiver requires paired topic/payload frames. Binding a specific
local IP such as `10.126.17.125` limits the publisher to that interface; binding
the receiver's `10.126.17.75` fails because it is not an address on the tracker.

The existing Python republisher binds `tcp://*:987x`. Stop only that rig's
republisher before binding the same port directly in HejjéTrac, or choose a
different unused port and update its receiver. A second publisher cannot share
an occupied bind port. The original alternative still works: keep direct ZMQ
disabled and send HejjéTrac's UDP FT rows to that rig's Python republisher.
Changing bind scope does not change payload bytes, framing, coordinates or the
bounded queue/peer behavior.

The same fields can be edited through the CLI or agent interfaces:

```sh
hejjetrac-cli config --config VR1.txt \
  --set zmq_output_mode=legacy_matrex_vr \
  --set zmq_framing=topic_json \
  --set zmq_topic=FT \
  --output VR1-matrex.txt
```

## Exact payload and framing

The conversion matches the [existing Python republisher](https://github.com/pvnkmrksk/fictrac/blob/10fc4472af1fc9062b9bf5a92738e835b98cc3ec/VR_array/socket_zmq_republisher.py).
Split the complete FT row with **`FT` at index 0**:

| JSON field | FT input |
|---|---|
| `x` | `fields[15]` |
| `y` | `fields[16]` |
| `yaw` | `fields[17]`, converted from radians to degrees |
| `z`, `pitch`, `roll` | `0.0` |

x/y remain cumulative displacement in sphere radians, with the original axes.
**Do not multiply by the sphere radius in HejjéTrac.** Unity applies it. There
is no axis swap, new origin, reset offset or reconstructed motion across a lost
tracking interval. Conversion uses the already formatted FT values, including
their decimal rounding. JSON key order, spaces, signed zero and floating-point
notation match `json.dumps`.

`single` sends one UTF-8 JSON frame. `topic_json` sends **one multipart message**
containing `[topic, JSON]`, not two separate messages. The topic defaults to `FT`;
both old MATREX players subscribe to all topics. Native FT subscribers can filter
on `FT, `; single-frame JSON subscribers need an empty prefix or `{` instead.
Only accepted, finite measurements are published. Recording keeps its normal
all-frame numeric/quality evidence independently of subscriber availability.

## Why a transport fix was required

The unchanged **NetMQ 4.0.0.1** assembly uses ZMTP 2.0 (greeting revision 1).
The former Rust backend waited for a 64-byte ZMTP 3 greeting, so a JSON-only
change could not establish a connection.

The new Rust publisher stages its greeting and negotiates the old 2.0 handshake
or its supported NULL-authenticated 3.0 protocol with newer peers, following
[ZMTP version negotiation](https://rfc.zeromq.org/spec/23/#version-negotiation).
This is a bounded TCP PUB transport with SUB/XSUB peers, not a general
implementation of every ZeroMQ socket type or authentication mechanism.

Each peer has its own queue and write deadline. Partial greetings, slow
receivers and malformed controls cannot block numerical processing or another
subscriber. Pre-subscription poses are discarded. Peer/queue limits, counter
semantics and shutdown behavior are in the [output reference](output.md).

## Validation against unchanged players

On Apple M2 Pro / macOS 27.0.1 arm64, the published **v0.1.0** and **v0.3.0**
universal Mac players ran with their original executable and managed assemblies,
including the original NetMQ 4.0.0.1 DLL. Only ordinary JSON experiment settings
in disposable copies were changed. Runtime hashes were checked before and after.
No receiver patch, DLL replacement, runtime injection or Python publisher was used.

All four version/framing combinations passed the functional checks:

| Unchanged player | JSON framing | Functional checks | Clean receiver lifecycle |
|---|---|---|---|
| v0.1.0 | Single | Pass | Existing worker-stop warnings |
| v0.1.0 | Topic + JSON | Pass | Existing worker-stop warnings |
| v0.3.0 | Single | Pass | Pass |
| v0.3.0 | Topic + JSON | Pass | Pass |

Each case used four rigs, six complete scene loads (five reloads), a synthetic
30 Hz trajectory, heading crossing 0/360 degrees, rejected marker rows and a
one-second publisher disconnect/rebind while retaining cumulative position.
Unity's recorded sensor values and world transforms verified movement, heading,
legacy axes/units and radius scaling in Unity only. The matrix contains
**33,300 CSV observations**, including repeated render-frame observations;
this count is not a wire-packet throughput measurement.

The exact old DLL also received Python-identical payload bytes in both framings
and reconnected to a restarted publisher. An independent modern libzmq subscriber
received **300/300 real sample rows** in each of native FT, single JSON and
multipart JSON, checking exact contents and order. Native FT and UDP remained
byte-identical. Separate actual-video recording tests cover an absent subscriber
and an incomplete TCP greeting, retaining all 300 numeric/quality rows.

- [Unchanged-player hashes, measurements and lifecycle results](benchmarks/matrex-legacy-20261026.json)
- [Exact old DLL and independent modern libzmq results](benchmarks/matrex-wire-20261026.json)

### Remaining old v0.1.0 lifecycle issue

The original v0.1.0 listener still reports 500 ms worker-stop timeouts: nine
and twelve warnings in the final single/multipart cases. Its older recording
shutdown can also report an archive-sharing error. Functional movement,
heading, reconnect and scene-reload checks pass, but **strict clean-lifecycle
acceptance remains false for that receiver**. Changing the publisher does not
repair its thread/socket teardown. Prior receiver diagnostics show retained
threads/descriptors; retained TCP carriers and long-run behavior at the
publisher's 64-peer limit were not measured. The six-load tests do not establish
unlimited scene-reset stability.

The standalone old-DLL probe also cannot claim clean NetMQ cleanup under .NET 7;
it records a bounded forced exit after successful data/reconnect checks. The
unchanged Unity players are the application-level acceptance evidence.

The player tests ran headless on Mac and checked recorded Unity transforms.
They do not establish Windows/Linux legacy-player execution, display hardware,
physical-camera tracking accuracy or sustained multi-camera capacity.

## Reproduce the integration tests

Build the Rust fixture:

```sh
cargo build -p fictrac-io --example matrex_legacy_fixture --release --locked
```

`scripts/test_matrex_legacy_players.py` accepts paths to the unchanged v0.1.0 and
v0.3.0 Mac archives, verifies their pinned SHA-256 hashes and runs the matrix.
It retains raw CSV/log/hash evidence and distinguishes functional compatibility
from clean teardown; its strict exit status remains failing when the old
v0.1.0 lifecycle warnings occur.

`scripts/test_netmq_legacy.py` requires a test-only .NET SDK and an externally
supplied old `NetMQ.dll`/`AsyncIO.dll` directory. It refuses a substituted NetMQ
assembly. `scripts/test_libzmq_output.py` is an independent subscriber oracle
using test-only PyZMQ. None of these tools is needed to run HejjéTrac or Unity.
