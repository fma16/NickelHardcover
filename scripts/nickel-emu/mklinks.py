#!/usr/bin/env python3
"""Creates the SONAME symlinks of an extracted Kobo rootfs, as ldconfig does on the device.

The firmware archive only updates an existing system: some links are absent and some point
to files it does not ship, so missing and dangling links are both (re)created."""
import os, subprocess, sys
root = os.path.abspath(sys.argv[1])
dirs = ["lib", "usr/lib", "usr/local/Kobo", "usr/local/Qt-5.2.1-arm/lib"]
made = 0
for d in dirs:
    full = os.path.join(root, d)
    if not os.path.isdir(full):
        continue
    for name in sorted(os.listdir(full)):
        path = os.path.join(full, name)
        if os.path.islink(path) or not os.path.isfile(path) or ".so" not in name:
            continue
        with open(path, "rb") as f:
            if f.read(4) != b"\x7fELF":
                continue
        out = subprocess.run(["arm-linux-gnueabihf-readelf", "-d", path], capture_output=True, text=True).stdout
        for line in out.splitlines():
            if "(SONAME)" in line:
                soname = line.split("[")[1].split("]")[0]
                link = os.path.join(full, soname)
                if soname != name and not os.path.exists(link):
                    if os.path.islink(link):
                        os.remove(link)
                    os.symlink(name, link)
                    made += 1
print("links created:", made)
