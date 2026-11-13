#!/bin/sh
# Stops the "nickel" service and every emulated ARM process it forked.
amp orb service stop nickel >/dev/null 2>&1
emulated() { ps -eo pid=,args= | awk '$2 == "/usr/bin/qemu-arm-static" { print $1 }'; }
emulated | xargs -r kill 2>/dev/null
sleep 1
emulated | xargs -r kill -9 2>/dev/null
true
