#!/usr/bin/env bash
# Prepares an emulated Kobo Libra Colour (firmware 4.46.23852) under $EMU (default ~/emu):
# orb packages, qemu-arm binfmt, Docker daemon, firmware rootfs, fake hardware and the
# LD_PRELOAD shim. Idempotent; it never touches an existing onboard/ (books, plugin data).
# Next: install.sh, mkconfig.sh, run.sh.
set -euo pipefail
D=$(dirname "$(readlink -f "$0")")
E=${EMU:-$HOME/emu}
FW_URL=https://ereaderfiles.kobo.com/firmwares/kobo13/Oct2026/kobo-update-4.46.23852.zip
FW_SHA256=3cf68dbae177c5402b5d2f033d0de1caf7aa30282e0d4f26972a04388e0f8791
R=$E/rootfs

missing=()
for package in qemu-user-static binutils-arm-linux-gnueabihf unzip; do
  if ! dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'install ok installed'; then
    missing+=("$package")
  fi
done
if ((${#missing[@]})); then
  sudo apt-get update -qq
  sudo apt-get install -y "${missing[@]}"
fi
command -v magick >/dev/null || echo 'warning: ImageMagick (magick) is missing; shot.sh needs it' >&2
"$D/binfmt.sh"
sudo docker info >/dev/null 2>&1 || sudo systemctl start docker

mkdir -p "$E/fw" "$E/logs"
zip=$E/fw/${FW_URL##*/}
if [[ ! -f $zip ]]; then
  curl -fL --retry 3 -sS -o "$zip.part" "$FW_URL"
  mv "$zip.part" "$zip"
fi
echo "$FW_SHA256  $zip" | sha256sum -c - >/dev/null

if [[ ! -x $R/usr/local/Kobo/nickel ]]; then
  unzip -o -q "$zip" KoboRoot.tgz -d "$E/fw"
  mkdir -p "$R"
  tar -xzf "$E/fw/KoboRoot.tgz" -C "$R" --no-same-owner
  python3 "$D/mklinks.py" "$R" # the device runs ldconfig; the chroot has no ld.so.cache
fi

# The real udev script unmounts /mnt/onboard and drives the USB gadget.
usb=$R/usr/local/Kobo/udev/usb
if [[ ! -f $usb.orig ]]; then
  mv "$usb" "$usb.orig"
  printf '%s\n' '#!/bin/sh' '# Emulator-only stub; the firmware script is usb.orig.' \
    'echo "emu usb stub ACTION=$ACTION" >> /emu/usb-stub.log' 'exit 0' > "$usb"
  chmod +x "$usb"
fi

EMU=$E "$D/mkfakesys.sh"
mkdir -p "$R/emu/input" "$E/fakedev/input"
for fifo in hwstatus input/event0 input/event1; do
  [[ -p $R/emu/$fifo ]] || mkfifo "$R/emu/$fifo"
done
ln -sfn /emu/input/event0 "$E/fakedev/input/event0"
ln -sfn /emu/input/event1 "$E/fakedev/input/event1"
# No Wi-Fi radio: while this file exists the shim tells the plugin the internet is reachable.
[[ -d $E/onboard ]] || : > "$R/emu/online"

# Nickel needs .kobo/version; SideloadedMode skips the Kobo account setup and
# ReadABookShown2 the first-book tutorial. Nickel creates the databases itself.
if [[ ! -d $E/onboard/.kobo ]]; then
  mkdir -p "$E/onboard/.kobo/Kobo"
  echo '112233445566,6.1.189+,4.46.23852,6.1.189+,6.1.189+,00000000-0000-0000-0000-000000000390' \
    > "$E/onboard/.kobo/version"
  printf '[ApplicationPreferences]\nCurrentLocale=en\nReadABookShown2=true\nSideloadedMode=true\n' \
    > "$E/onboard/.kobo/Kobo/Kobo eReader.conf"
fi

EMU=$E "$D/build-shim.sh"
echo "Emulated Kobo ready in $E"
