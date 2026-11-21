#!/usr/bin/env python3
"""Lists the globals a Wayland compositor offers: wlglobals.py [SOCKET_NAME]."""
import os, socket, struct, sys
name = sys.argv[1] if len(sys.argv) > 1 else os.environ.get('WAYLAND_DISPLAY', 'wayland-0')
path = name if name.startswith('/') else os.path.join(os.environ['XDG_RUNTIME_DIR'], name)
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.settimeout(3); s.connect(path)
# wl_display.get_registry(new_id 2), wl_display.sync(new_id 3)
s.sendall(struct.pack('<IHHI', 1, 1, 12, 2) + struct.pack('<IHHI', 1, 0, 12, 3))
buf = b''; done = False; found = []
while not done:
    data = s.recv(65536)
    if not data: break
    buf += data
    while len(buf) >= 8:
        obj, op, size = struct.unpack_from('<IHH', buf)
        if len(buf) < size: break
        body = buf[8:size]; buf = buf[size:]
        if obj == 2 and op == 0:
            gid, slen = struct.unpack_from('<II', body)
            iface = body[8:8 + slen - 1].decode()
            ver = struct.unpack_from('<I', body, 8 + ((slen + 3) & ~3))[0]
            found.append((iface, ver))
        elif obj == 3: done = True
for iface, ver in sorted(found): print(f'{iface} v{ver}')
