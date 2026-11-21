#!/usr/bin/env python3
"""Check that a web process showing an ordinary page has not loaded the graphics driver.

The GPU process paints and composites pages and creates their swap chain
buffers, so a web process needs neither EGL nor GBM until a page uses WebGL or
plays video. This opens a static local page, waits for it to be painted, and
then looks at the memory mappings of every web process of the browser: none may
map the driver (Mesa's libgallium, or the libLLVM it brings along), and the page
must be on screen, so that a browser that drew nothing does not pass.

It needs a benchmark build (see docs/development.md), `grim`, and the graphical
session of `tools/benchmark_sites.py`, which it shares.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)

DRIVER_LIBRARIES = ('libgallium', 'libLLVM')
# Two solid halves, so that the capture can tell the page from an unpainted view.
PAGE = b'''<!doctype html><meta charset=utf-8><title>graphics check</title>
<style>html,body{margin:0;height:100%}div{height:50%}#a{background:#1d6fd0}#b{background:#d06f1d}</style>
<div id=a></div><div id=b></div>'''
EXPECTED = ((0x1d, 0x6f, 0xd0), (0xd0, 0x6f, 0x1d))


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(PAGE)))
        self.end_headers()
        self.wfile.write(PAGE)


def web_processes(unit):
    """Each web process of the unit's control group with the driver libraries it maps."""
    group = subprocess.run(['systemctl', '--user', 'show', '-p', 'ControlGroup', '--value', unit],
                           capture_output=True, text=True, check=True).stdout.strip()
    found = {}
    for pid in Path('/sys/fs/cgroup' + group, 'cgroup.procs').read_text().split():
        try:
            if Path('/proc', pid, 'comm').read_text().strip() != 'WPEWebProcess':
                continue
            maps = Path('/proc', pid, 'maps').read_text()
        except OSError:
            continue
        found[pid] = sorted({name for name in DRIVER_LIBRARIES if f'/{name}' in maps})
    return found


def painted(session):
    """Whether the two colours of the page are on screen, one above the other."""
    from PIL import Image
    capture = subprocess.run(['grim', '-t', 'ppm', '-'], env=session, capture_output=True, check=True).stdout
    image = Image.open(io.BytesIO(capture)).convert('RGB')
    width, height = image.size

    def close(pixel, colour):
        return all(abs(a - b) <= 12 for a, b in zip(pixel, colour))
    column = [image.getpixel((width // 2, y)) for y in range(height)]
    upper = [y for y, pixel in enumerate(column) if close(pixel, EXPECTED[0])]
    lower = [y for y, pixel in enumerate(column) if close(pixel, EXPECTED[1])]
    return len(upper) > height // 8 and len(lower) > height // 8 and max(upper) < min(lower)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    parser.add_argument('--runtime', default=str(benchmark_sites.ROOT / benchmark_sites.DEPENDENCIES['native_runtime']['path']))
    parser.add_argument('--source-pin', type=Path)
    parser.add_argument('--env', action='append', default=[], metavar='K=V')
    arguments = parser.parse_args()

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    environment = benchmark_sites.desk()
    session = {**os.environ, **environment}
    session.pop('DISPLAY', None)
    with tempfile.TemporaryDirectory(prefix='web-process-graphics-') as scratch:
        options = argparse.Namespace(runtime=arguments.runtime, source_pin=arguments.source_pin, enzim_binary=arguments.enzim_binary,
                                     env=arguments.env, resources=False, memory_max='6G')
        browser = benchmark_sites.Enzim(options, Path(scratch), environment)
        try:
            browser.start()
            time.sleep(3)
            browser.open(f'http://127.0.0.1:{server.server_address[1]}/')
            deadline = time.monotonic() + 15
            shown = False
            while not shown and time.monotonic() < deadline:
                time.sleep(0.5)
                shown = painted(session)
            processes = web_processes(browser.unit)
        finally:
            browser.stop()
    server.shutdown()

    with_driver = {pid: libraries for pid, libraries in processes.items() if libraries}
    checks = {
        'the page is on screen': shown,
        'the browser has web processes': bool(processes),
        'no web process maps the graphics driver': not with_driver,
    }
    for name, passed in checks.items():
        print(f'{"ok    " if passed else "FAILED"} {name}')
    if with_driver:
        print('web processes with the driver:', with_driver)
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
