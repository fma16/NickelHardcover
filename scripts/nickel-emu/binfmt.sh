#!/bin/sh
# Registers qemu-arm-static for ARM ELF binaries in this orb (needs qemu-user-static).
# Flag F keeps the interpreter open, so it also works inside a chroot/user namespace.
set -e
[ -e /proc/sys/fs/binfmt_misc/qemu-arm ] && exit 0
printf '%s' ':qemu-arm:M::\x7fELF\x01\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x28\x00:\xff\xff\xff\xff\xff\xff\xff\x00\xff\xff\xff\xff\xff\xff\xff\xff\xfe\xff\xff\xff:/usr/bin/qemu-arm-static:F' | sudo tee /proc/sys/fs/binfmt_misc/register >/dev/null
