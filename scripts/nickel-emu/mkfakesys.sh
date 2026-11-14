#!/bin/sh
# Synthetic /sys for the emulated Kobo (Libra Colour, mt8113t-ntx): a battery so Nickel does not power off.
S=${EMU:-$HOME/emu}/fakesys
B=$S/class/power_supply/bd71827_bat
mkdir -p "$B" "$S/class/backlight/lm3630a_led" "$S/class/leds/LED" "$S/devices/system/cpu/cpu0" \
  "$S/devices/system/cpu/cpufreq/policy0" "$S/devices/platform/gpio-keys" "$S/class/input" "$S/class/rfkill"
echo "${EMU_BATTERY:-87}" > "$B/capacity"
echo Discharging > "$B/status"
echo 0 > "$B/charging"
echo 10 > "$S/class/backlight/lm3630a_led/max_color"
echo 0 > "$S/class/backlight/lm3630a_led/color"
echo 0 > "$S/class/leds/LED/brightness"
echo 0 > "$S/devices/system/cpu/possible"
echo 1 > "$S/devices/system/cpu/cpu0/online"
echo interactive > "$S/devices/system/cpu/cpufreq/policy0/scaling_governor"
echo 0 > "$S/devices/platform/gpio-keys/PageUp.deepslp_wake_en"
echo 0 > "$S/devices/platform/gpio-keys/PageDown.deepslp_wake_en"
# Input devices laid out like real sysfs, because Nickel's DeviceDiscoverer walks them with libudev
# (capabilities are 32-bit hex words, highest first).
rm -rf "$S/class/input" "$S/devices/virtual/input"
mkdir -p "$S/class/input" "$S/devices/virtual/input" "$S/bus"
mkinput() { # index name ev key abs
  d=$S/devices/virtual/input/input$1
  mkdir -p "$d/capabilities" "$d/event$1"
  echo "$2" > "$d/name"
  echo "$3" > "$d/capabilities/ev"; echo "$4" > "$d/capabilities/key"
  echo "$5" > "$d/capabilities/abs"; echo 0 > "$d/capabilities/msc"
  printf 'PRODUCT=0/0/0/0\nNAME="%s"\n' "$2" > "$d/uevent"
  printf 'MAJOR=13\nMINOR=%s\nDEVNAME=input/event%s\n' $((64 + $1)) "$1" > "$d/event$1/uevent"
  echo "13:$((64 + $1))" > "$d/event$1/dev"
  ln -sfn ../../../../class/input "$d/subsystem"
  ln -sfn ../../../../../class/input "$d/event$1/subsystem"
  ln -sfn ../../input$1 "$d/event$1/device"
  ln -sfn ../../devices/virtual/input/input$1 "$S/class/input/input$1"
  ln -sfn ../../devices/virtual/input/input$1/event$1 "$S/class/input/event$1"
}
# event0: power key (116) and page buttons F23/F24 (193/194)
mkinput 0 ntx_event0 3 "6 0 0 100000 0 0 0" 0
# event1: multitouch type B panel: BTN_TOUCH; ABS_X, ABS_Y, MT slot/major/position/tracking id/pressure
mkinput 1 cyttsp5_mt b "400 0 0 0 0 0 0 0 0 0 0" "6618000 3"
