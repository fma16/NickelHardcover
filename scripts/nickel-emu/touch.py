#!/usr/bin/env python3
"""Injects touches into the emulated Kobo (multitouch type B on /dev/input/event1).

usage: touch.py tap X Y [hold_ms] | swipe X1 Y1 X2 Y2 [ms] | key CODE
Coordinates are screen pixels of the upright 1264x1680 screenshot.
"""
import os, struct, sys, time
EMU = os.environ.get('EMU', os.path.expanduser('~/emu'))
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
SLOT, MAJOR, X, Y, TRACK, PRESSURE, BTN_TOUCH = 0x2f, 0x30, 0x35, 0x36, 0x39, 0x3a, 0x14a

def emit(fd, events):
    now = time.time()
    data = b''.join(struct.pack('<IIHHi', int(now), int(now % 1 * 1e6), t, c, v) for t, c, v in events)
    os.write(fd, data)

def dev(x, y):  # inverse of Nickel's monzaMirrorYSwapMapper: screen = (devY, 1680 - devX)
    return 1680 - y, x

def down(fd, x, y, first):
    dx, dy = dev(x, y)
    ev = [(EV_ABS, SLOT, 0), (EV_ABS, TRACK, int(time.time() * 10) % 60000)] if first else []
    ev += [(EV_ABS, X, dx), (EV_ABS, Y, dy), (EV_ABS, MAJOR, 10), (EV_ABS, PRESSURE, 60)]
    if first: ev.append((EV_KEY, BTN_TOUCH, 1))
    emit(fd, ev + [(EV_SYN, 0, 0)])

def up(fd):
    emit(fd, [(EV_ABS, SLOT, 0), (EV_ABS, TRACK, -1), (EV_KEY, BTN_TOUCH, 0), (EV_SYN, 0, 0)])

def main(a):
    if a[0] == 'key':
        fd = os.open(f'{EMU}/rootfs/emu/input/event0', os.O_WRONLY | os.O_NONBLOCK)
        emit(fd, [(EV_KEY, int(a[1]), 1), (EV_SYN, 0, 0)]); time.sleep(0.1)
        emit(fd, [(EV_KEY, int(a[1]), 0), (EV_SYN, 0, 0)]); return
    fd = os.open(f'{EMU}/rootfs/emu/input/event1', os.O_WRONLY | os.O_NONBLOCK)
    n = [int(v) for v in a[1:]]
    if a[0] == 'tap':
        down(fd, n[0], n[1], True); time.sleep((n[2] if len(n) > 2 else 80) / 1000); up(fd)
    elif a[0] == 'swipe':
        ms = n[4] if len(n) > 4 else 300; steps = 12
        for i in range(steps + 1):
            down(fd, n[0] + (n[2] - n[0]) * i // steps, n[1] + (n[3] - n[1]) * i // steps, i == 0)
            time.sleep(ms / 1000 / steps)
        up(fd)

main(sys.argv[1:])
