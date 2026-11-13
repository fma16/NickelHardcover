#!/bin/sh
# Runs a command inside the running emulation (same namespaces and chroot as Nickel).
# usage: in.sh <command> [args...]
E=${EMU:-$HOME/emu}
P=$(pgrep -f 'qemu-arm-static ./nickel' | head -1)
[ -n "$P" ] || { echo "nickel is not running" >&2; exit 1; }
exec nsenter -t "$P" -U -m --preserve-credentials /usr/sbin/chroot "$E/rootfs" "$@"
