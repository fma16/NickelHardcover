#!/bin/sh
# Starts emulated Nickel as the supervised orb service "nickel" (log: $EMU/logs/nickel.log). stop.sh ends it.
E=${EMU:-$HOME/emu}; D=$(dirname "$(readlink -f "$0")")
"$D/stop.sh"
rm -f "$E/rootfs/emu/shim.log" "$E/rootfs/emu/fb.raw"
cp "$D/emu-start.sh" "$E/rootfs/emu-start.sh"
exec amp orb service start nickel --command "EMU=$E exec $D/enter.sh /emu-start.sh > $E/logs/nickel.log 2>&1"
