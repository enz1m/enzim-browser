#!/usr/bin/env python3
"""Check that a revisited page paints before its deferred script runs.

When a page's deferred scripts are already loaded, as on a revisit served from
the cache, they could run the moment parsing ends, and the first paint would
wait for all of them. The engine gives a visible page with content up to
100 ms to paint first. This serves a page of two hundred paragraphs with a
cacheable deferred script that keeps the main thread busy for 200 ms, loads it
twice, and compares the second load's first contentful paint with its
DOMContentLoaded event, which follows the script.

With the wait the paint must come at least 100 ms before DOMContentLoaded.
With `WPE_PAINT_BEFORE_DEFERRED_SCRIPTS=0` it must not: a fixture whose control
paints early anyway would let a broken wait pass, and is reported as such.

It needs a benchmark build (see docs/development.md) and the graphical session
of `tools/benchmark_sites.py`, which it shares.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)

BUSY_MS = 200
PAGE = ('<!doctype html><meta charset=utf-8><title>deferred script check</title>'
        '<style>body{font:16px sans-serif;margin:24px}</style>'
        '<script defer src="/work.js"></script>'
        + ''.join(f'<p>Paragraph {index} of the page that should be seen before its script has run.</p>'
                  for index in range(200))).encode()
SCRIPT = f'const until = performance.now() + {BUSY_MS}; while (performance.now() < until) {{}}'.encode()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        script = self.path == '/work.js'
        body = SCRIPT if script else PAGE
        self.send_response(200)
        self.send_header('Content-Type', 'text/javascript' if script else 'text/html; charset=utf-8')
        self.send_header('Cache-Control', 'public, max-age=600' if script else 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def second_load(arguments, environment, address, extra):
    """First contentful paint and DOMContentLoaded of the page's second load, on its own clock."""
    with tempfile.TemporaryDirectory(prefix='paint-before-deferred-') as scratch:
        options = argparse.Namespace(runtime=arguments.runtime, source_pin=arguments.source_pin, enzim_binary=arguments.enzim_binary,
                                     env=[*arguments.env, *extra], resources=False, memory_max='6G')
        browser = benchmark_sites.Enzim(options, Path(scratch), environment)
        try:
            browser.start()
            time.sleep(3)
            result = None
            for _ in range(2):
                requested = browser.open(address)
                deadline = time.monotonic() + 20
                result = None
                while result is None and time.monotonic() < deadline:
                    time.sleep(0.1)
                    current = browser.poll()
                    if current and current.get('origin', 0) / 1000 >= requested - 1 and current.get('load') and current.get('fcp'):
                        result = current['fcp'], current['dcl']
                time.sleep(1.5)
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
    address = f'http://127.0.0.1:{server.server_address[1]}/'
    environment = benchmark_sites.desk()
    waited = second_load(arguments, environment, address, [])
    control = second_load(arguments, environment, address, ['WPE_PAINT_BEFORE_DEFERRED_SCRIPTS=0'])
    server.shutdown()

    def early(result):
        return result is not None and result[1] - result[0] >= BUSY_MS / 2
    # A control that paints before the script anyway shows nothing about the wait.
    conclusive = control is not None and not early(control)
    checks = {
        'the page loads twice with the wait and without it': waited is not None and control is not None,
        'without the wait the revisit paints after its deferred script': conclusive,
        'with the wait the revisit paints before its deferred script': early(waited),
    }
    for name, passed in checks.items():
        print(f'{"ok    " if passed else "FAILED"} {name}')
    print(f'second load (first contentful paint, DOMContentLoaded) in ms: with the wait {waited}, without {control}')
    if not conclusive:
        print('The control painted before its script: the fixture no longer shows the old order.')
        return 2
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
