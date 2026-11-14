#!/bin/sh
# Runs a command inside the extracted Kobo rootfs, as fake root in a private
# user+mount namespace (no real privileges on the orb). ARM binaries run through
# the qemu-arm binfmt handler registered by binfmt.sh.
# usage: enter.sh <command> [args...]      env: EMU (default ~/emu)
EMU=${EMU:-$HOME/emu}
R=$EMU/rootfs
mkdir -p "$EMU/onboard" "$EMU/fakesys" "$EMU/fakedev"
for d in proc dev sys tmp mnt/onboard mnt/sd var/run var/lib var/log var/db root data; do mkdir -p "$R/$d"; done
exec unshare -rm sh -c '
  R=$1; EMU=$2; shift 2
  mount --rbind /proc "$R/proc"
  mount -t tmpfs none "$R/dev"
  for n in null zero urandom random tty; do : > "$R/dev/$n"; mount --bind /dev/$n "$R/dev/$n"; done
  mkdir -p "$R/dev/shm" "$R/dev/input"
  cp -a "$EMU/fakedev/." "$R/dev/" 2>/dev/null
  mount -t tmpfs none "$R/tmp"
  mount -t tmpfs none "$R/var/run"
  mount -t tmpfs none "$R/var/log"
  mount --bind "$EMU/fakesys" "$R/sys"
  mount --bind "$EMU/onboard" "$R/mnt/onboard"
  cp /etc/resolv.conf "$R/etc/resolv.conf" 2>/dev/null
  exec /usr/sbin/chroot "$R" "$@"
' sh "$R" "$EMU" "$@"
