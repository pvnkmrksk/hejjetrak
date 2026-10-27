#!/bin/sh
# Use the calibrated repo rigs and the existing UDP-to-ZMQ republishers.
set -eu

hejjetrac_repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
hejjetrac_binary=${HEJJETRAC_BINARY:-"$hejjetrac_repo/dist/hejjetrac-linux-x86_64/hejjetrac"}
if [ ! -x "$hejjetrac_binary" ]; then
    hejjetrac_binary="$hejjetrac_repo/target/release/fictrac-desktop"
fi
if [ ! -x "$hejjetrac_binary" ]; then
    echo "Cannot find HejjeTrac. Extract the portable build into dist/hejjetrac-linux-x86_64 or build the release binary." >&2
    exit 1
fi

exec "$hejjetrac_binary" --config "$hejjetrac_repo/VR1.txt" --windows 4 --grid 2x2 "$@"
