#!/bin/sh
# Runs a command in the Kobo cross toolchain image, from the repository root, with the
# repository and $EMU mounted at their host paths.
# usage: tc.sh '<command>'
# env: TC_IMAGE (default: the NickelTC base image; use bookwise-nickeltc for mise and Rust),
#      TC_ENV (extra docker arguments, e.g. "-e VERSION=…")
D=$(dirname "$(readlink -f "$0")"); ROOT=$(cd "$D/../.." && pwd)
E=${EMU:-$HOME/emu}; mkdir -p "$E"
exec sudo docker run --rm -v "$ROOT:$ROOT" -v "$E:$E" -w "$ROOT" -u "$(id -u):$(id -g)" -e HOME=/tmp \
  ${TC_ENV:-} "${TC_IMAGE:-ghcr.io/pgaskin/nickeltc:1.0-9779f94.19}" sh -c "$*"
