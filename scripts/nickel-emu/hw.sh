#!/bin/sh
# Sends a hardware event line to emulated Nickel, e.g. hw.sh usb plug add | hw.sh usb plug remove
E=${EMU:-$HOME/emu}
timeout 5 sh -c 'echo "$1" > "$2"' sh "$*" "$E/rootfs/emu/hwstatus"
