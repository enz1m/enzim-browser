#!/usr/bin/env python3
"""Captures KWin's workspace to a PNG through org.kde.KWin.ScreenShot2: capture.py OUT.png (run inside the container by tools/glass_desk/kwin)"""
import os, sys, dbus
from PIL import Image
bus = dbus.SessionBus()
shots = dbus.Interface(bus.get_object('org.kde.KWin', '/org/kde/KWin/ScreenShot2'), 'org.kde.KWin.ScreenShot2')
read, write = os.pipe()
result = shots.CaptureWorkspace({'native-resolution': dbus.Boolean(True, variant_level=1)}, dbus.types.UnixFd(write))
os.close(write)
width, height, stride = int(result['width']), int(result['height']), int(result['stride'])
data = b''
while len(data) < stride * height:
    chunk = os.read(read, 1 << 20)
    if not chunk: break
    data += chunk
Image.frombuffer('RGBA', (width, height), data, 'raw', 'BGRA', stride, 1).convert('RGB').save(sys.argv[1])
print(width, height, int(result['format']))
