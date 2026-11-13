#!/bin/sh
# Runs taps in sequence, waiting wait_s (default 4) after each: seq.sh "X Y wait_s" "X Y wait_s" ...
D=$(dirname "$(readlink -f "$0")")
for a in "$@"; do set -- $a; "$D/touch.py" tap "$1" "$2"; sleep "${3:-4}"; done
