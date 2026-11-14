#!/usr/bin/env bash
# Installs a plugin archive into the emulated Kobo the way Nickel unpacks KoboRoot.tgz.
# Restart Nickel (run.sh) afterwards: the plugin is loaded at startup.
# usage: install.sh [KoboRoot.tgz]   (default: the archive at the repository root)
set -euo pipefail
D=$(dirname "$(readlink -f "$0")")
E=${EMU:-$HOME/emu}
archive=${1:-$D/../../KoboRoot.tgz}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
tar -xzf "$archive" -C "$tmp"
install -D "$tmp/usr/local/Kobo/imageformats/libhardcover.so" "$E/rootfs/usr/local/Kobo/imageformats/libhardcover.so"
adds=$E/onboard/.adds/NickelHardcover
mkdir -p "$adds"
cp -f "$tmp/mnt/onboard/.adds/NickelHardcover/"* "$adds/"
# The hook starts ".adds/NickelHardcover/CLI" (hook/src/files.h) while the archive ships
# "cli": the same file on the Kobo's FAT storage, two different names here.
cp -f "$adds/cli" "$adds/CLI"
ls -l "$E/rootfs/usr/local/Kobo/imageformats/libhardcover.so" "$adds"
