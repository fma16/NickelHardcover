#!/bin/sh
# Writes the plugin config for the emulated Kobo from env tokens (never echoed, never stored in the repo).
# usage: mkconfig.sh [provider]   (default dual)
E=${EMU:-$HOME/emu}; D=$E/onboard/.adds/NickelHardcover
mkdir -p "$D"; umask 077
{
  echo "provider = ${1:-dual}"
  echo "readwise_token = $Readwise_test_account"
  echo "access_token = $Hardcover_test_account"
  echo "readwise_publish_to_feed = false"
  echo "readwise_sync_annotations = false"
  echo "auto_sync_default = false"
  echo "journal_privacy = private"
  echo "retry_on_network = true"
  echo "sync_annotations = false"
  echo "sync_on_close = 1"
  echo "sync_on_read = 20"
  echo "sync_on_schedule = -1"
  echo "debug = true"
} > "$D/config.ini"
echo "config written ($(wc -l < "$D/config.ini") lines)"
