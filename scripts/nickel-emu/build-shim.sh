#!/bin/sh
# Cross-compiles the test-only hardware shim (emushim.c) into the emulated rootfs.
D=$(dirname "$(readlink -f "$0")"); E=${EMU:-$HOME/emu}
mkdir -p "$E/rootfs/emu"
exec "$D/tc.sh" "arm-nickel-linux-gnueabihf-gcc -std=gnu99 -O2 -Wall -fPIC -shared -o $E/rootfs/emu/emushim.so $D/emushim.c -ldl"
