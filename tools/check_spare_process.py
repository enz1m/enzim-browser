#!/usr/bin/env python3
"""Check that a page moving to a new web process does not wait for a launch.

A response with `Cross-Origin-Opener-Policy` moves its page to a new web
process, and so does a navigation to another site. The browser asks the
engine to keep a spare process for the profile in use, so the move takes one
that is already running. Otherwise the response waits for process startup
before reaching the page's parser.

This serves a small page, with and without the header, opens the plain
page, then the other one three times in new tabs, two seconds apart so that a
spare can be made again. For a local document of a few kilobytes the time from
its first byte to its last is the wait for the process. The median must stay
under 30 ms.

It needs a benchmark build (see docs/development.md) and the graphical session
of `tools/benchmark_sites.py`, which it shares.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import statistics
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)

LIMIT_MS = 30
PAGE = ('<!doctype html><meta charset=utf-8><title>spare process check</title>'
        + ''.join(f'<p>Paragraph {index}.</p>' for index in range(100))).encode()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Cache-Control', 'no-store')
        if self.path.startswith('/isolated'):
            self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Content-Length', str(len(PAGE)))
        self.end_headers()
        self.wfile.write(PAGE)


def waits(arguments, environment, port):
    """First byte to last byte, in ms, of each navigation that moves to a new process."""
    with tempfile.TemporaryDirectory(prefix='spare-process-') as scratch:
        options = argparse.Namespace(runtime=arguments.runtime, source_pin=arguments.source_pin, enzim_binary=arguments.enzim_binary,
                                     env=arguments.env, resources=False, memory_max='6G')
        browser = benchmark_sites.Enzim(options, Path(scratch), environment)
        try:
            browser.start()
            time.sleep(3)
            browser.open(f'http://127.0.0.1:{port}/plain')
            time.sleep(3)
            for index in range(3):
                browser.open(f'http://127.0.0.1:{port}/isolated?n={index}')
                time.sleep(2.5)
            time.sleep(1)
            result = []
            for line in (Path(scratch) / 'browser.log').read_text(errors='replace').splitlines():
                if line.startswith('DOCUMENT_TIMING ') and '"phase":"load"' in line and '/isolated' in line:
                    navigation = json.loads(line[len('DOCUMENT_TIMING '):], strict=False)['payload'].get('navigation') or {}
                    result.append(navigation.get('responseEnd', 0) - navigation.get('responseStart', 0))
            return result
        finally:
            browser.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    parser.add_argument('--runtime', default=str(benchmark_sites.ROOT / benchmark_sites.DEPENDENCIES['native_runtime']['path']))
    parser.add_argument('--source-pin', type=Path)
    parser.add_argument('--env', action='append', default=[], metavar='K=V')
    arguments = parser.parse_args()

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    measured = waits(arguments, benchmark_sites.desk(), server.server_address[1])
    server.shutdown()

    checks = {
        'the isolated page loads three times': len(measured) == 3,
        f'its last byte follows its first within {LIMIT_MS} ms (median)': len(measured) == 3 and statistics.median(measured) < LIMIT_MS,
    }
    for name, passed in checks.items():
        print(f'{"ok    " if passed else "FAILED"} {name}')
    print('first byte to last byte in ms:', [round(value, 1) for value in measured])
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
