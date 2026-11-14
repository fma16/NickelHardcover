#!/bin/sh
# Prints the plugin's own syslog lines from the running emulation (NickelHook init noise removed).
D=$(dirname "$(readlink -f "$0")")
"$D/in.sh" /sbin/logread 2>&1 | grep -a 'NickelHardcover' | grep -a -v 'NickelHook' | cat -v | cut -c1-${COLS:-260} | tail -${1:-15}
