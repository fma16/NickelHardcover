#!/bin/sh
# Starts genuine Nickel inside the chroot (copy to <rootfs>/emu-start.sh).
export PATH=/sbin:/bin:/usr/sbin:/usr/bin
export PLATFORM=mt8113t-ntx PRODUCT=monza INTERFACE=wlan0 WIFI_MODULE=wlan_drv_gen4m
export NICKEL_HOME=/mnt/onboard/.kobo LD_LIBRARY_PATH=/usr/local/Kobo LANG=en_US.UTF-8
export KOBO_DEBUG_STDOUT=1
[ -n "$EMU_QT_DEBUG" ] && export QT_DEBUG_PLUGINS=1
if [ -z "$EMU_NO_SHIM" ] && [ -f /emu/emushim.so ]; then
  export EMU_SHIM_LOG=${EMU_SHIM_LOG:-/emu/shim.log} EMU_FB=${EMU_FB:-/emu/fb.raw}
  export LD_PRELOAD=/emu/emushim.so
fi
# rcS creates this FIFO on a device; udev scripts write "usb plug add" etc. to it.
ln -sf /emu/hwstatus /tmp/nickel-hardware-status
# NickelHook plugins log to syslog; keep it in a file readable from the orb.
: > /emu/syslog.log; /sbin/syslogd -O /emu/syslog.log -s 0
cd /usr/local/Kobo
# The device passes -skipFontLoad because hindenburg preloads fonts; here Nickel loads them itself.
exec ./nickel -platform ${EMU_PLATFORM:-kobo} ${EMU_NICKEL_ARGS:-} "$@"
