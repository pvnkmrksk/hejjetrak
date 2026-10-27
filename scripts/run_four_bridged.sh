#!/bin/sh
# Launch calibrated rigs through existing UDP-to-ZMQ republishers.
set -eu

hejjetrac_repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
if [ -n "${HEJJETRAC_BINARY:-}" ]; then
    hejjetrac_binary=$HEJJETRAC_BINARY
elif [ -x "$hejjetrac_repo/dist/hejjetrac-linux-x86_64/hejjetrac" ]; then
    hejjetrac_binary="$hejjetrac_repo/dist/hejjetrac-linux-x86_64/hejjetrac"
else
    hejjetrac_binary="$hejjetrac_repo/target/release/fictrac-desktop"
fi
if [ ! -x "$hejjetrac_binary" ]; then
    echo "Cannot find HejjéTrac. Extract the portable build into dist/hejjetrac-linux-x86_64, build the release binary, or set HEJJETRAC_BINARY." >&2
    exit 1
fi

hejjetrac_cli=${HEJJETRAC_CLI:-}
if [ -z "$hejjetrac_cli" ]; then
    hejjetrac_binary_dir=$(CDPATH= cd -- "$(dirname -- "$hejjetrac_binary")" && pwd)
    for hejjetrac_candidate in \
        "$hejjetrac_binary_dir/hejjetrac-cli" \
        "$hejjetrac_binary_dir/hejjetrak-cli" \
        "$hejjetrac_binary_dir/fictrac" \
        "$hejjetrac_repo/target/release/fictrac"; do
        if [ -x "$hejjetrac_candidate" ]; then
            hejjetrac_cli=$hejjetrac_candidate
            break
        fi
    done
fi
if [ ! -x "$hejjetrac_cli" ]; then
    echo "Cannot find the paired HejjéTrac CLI. Keep hejjetrac-cli beside the app, build target/release/fictrac, or set HEJJETRAC_CLI." >&2
    exit 1
fi

# Sibling VR names let the native multi-rig loader find each calibrated copy.
# The originals keep their direct publisher ports and are never edited.
hejjetrac_temporary=$(mktemp -d "${TMPDIR:-/tmp}/hejjetrac-bridged.XXXXXXXX")
hejjetrac_child=
hejjetrac_cleanup() {
    if [ -n "$hejjetrac_child" ]; then
        kill -s TERM "$hejjetrac_child" 2>/dev/null || :
        wait "$hejjetrac_child" 2>/dev/null || :
    fi
    rm -rf -- "$hejjetrac_temporary"
}
hejjetrac_signal() {
    trap - HUP INT TERM
    if [ -n "$hejjetrac_child" ]; then
        # Noninteractive shells can start background jobs with SIGINT ignored.
        # Translate Ctrl-C to TERM so the owned app still exits before cleanup.
        hejjetrac_forward=$1
        if [ "$hejjetrac_forward" = INT ]; then
            hejjetrac_forward=TERM
        fi
        kill -s "$hejjetrac_forward" "$hejjetrac_child" 2>/dev/null || :
        wait "$hejjetrac_child" 2>/dev/null || :
        hejjetrac_child=
    fi
    exit "$2"
}
trap hejjetrac_cleanup 0
trap 'hejjetrac_signal HUP 129' HUP
trap 'hejjetrac_signal INT 130' INT
trap 'hejjetrac_signal TERM 143' TERM

for hejjetrac_rig in 1 2 3 4; do
    "$hejjetrac_cli" config \
        --config "$hejjetrac_repo/VR$hejjetrac_rig.txt" \
        --set zmq_port=0 \
        --output "$hejjetrac_temporary/VR$hejjetrac_rig.txt"
done

"$hejjetrac_binary" --config "$hejjetrac_temporary/VR1.txt" --windows 4 --grid 2x2 "$@" &
hejjetrac_child=$!
hejjetrac_result=0
wait "$hejjetrac_child" || hejjetrac_result=$?
hejjetrac_child=
exit "$hejjetrac_result"
