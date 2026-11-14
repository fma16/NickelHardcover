#!/bin/sh
# usage: shot.sh <out.png>   Converts the emulated framebuffer (1264x1680 BGRA) to a PNG.
E=${EMU:-$HOME/emu}
magick -size ${EMU_FB_GEOM:-1264x1680} -depth 8 "bgra:$E/rootfs/emu/fb.raw[0]" -alpha off -rotate ${EMU_FB_ROT:-0} "$1"
