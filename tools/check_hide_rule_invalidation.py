#!/usr/bin/env python3
"""Check that style follows class, id and attribute changes against the ad list's hide rules.

The engine keeps the id and class rule tables of the content blocker's user
style out of each style scope's merged features. This page changes classes,
ids and attributes that only those hide rules name, in the document and in a
shadow tree, and reads the computed style before and after each change. It
needs a benchmark build (see docs/development.md) and the graphical session of
`tools/benchmark_sites.py`, which drives the browser.
"""
import argparse
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]
PAGE = (ROOT / 'tools/fixtures/hide_rule_invalidation.html').read_bytes()
CASES = ['class', 'id', 'descendant', 'child', 'idchild', 'attribute', 'compound', 'subject',
         'shadow-class', 'shadow-descendant', 'batch']
# Shown before the change, hidden after it, shown again when it is undone; and an
# element that carries a listed class from the start is hidden.
EXPECTED = {name: '111' for name in CASES} | {'static': 'hidden'}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        body = PAGE if self.path.split('?')[0] == '/' else b''
        self.send_response(200 if body else 404)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    parser.add_argument('--runtime', type=Path)
    parser.add_argument('--source-pin', type=Path)
    parser.add_argument('--env', action='append', default=[], metavar='K=V')
    arguments = parser.parse_args()
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix='hide-rules-') as scratch:
        scratch = Path(scratch)
        # The filters reach a new profile's first page shortly after it starts, hence the delay.
        url = f'http://127.0.0.1:{server.server_address[1]}/?delay=1500'
        (scratch / 'sites.json').write_text(json.dumps([{'name': 'Hides', 'url': url, 'title': 'RESULT'}]))
        command = [sys.executable, str(ROOT / 'tools/benchmark_sites.py'), '--browsers', 'enzim',
                   '--enzim-binary', str(arguments.enzim_binary), '--sites', str(scratch / 'sites.json'),
                   '--rounds', '1', '--settle', '4', '--no-screenshots', '--output', str(scratch / 'out')]
        for name in ('runtime', 'source_pin'):
            if getattr(arguments, name):
                command += ['--' + name.replace('_', '-'), str(getattr(arguments, name))]
        for pair in arguments.env:
            command += ['--env', pair]
        subprocess.run(command, check=True, stdout=subprocess.DEVNULL, timeout=180)
        result = json.loads((scratch / 'out/0-enzim/result.json').read_text())
        result = result if isinstance(result, list) else result['sites']
        title = (result[0].get('metrics') or {}).get('title') or ''
    server.shutdown()
    observed = dict(part.split(':', 1) for part in title.removeprefix('RESULT').split())
    failed = [name for name, value in EXPECTED.items() if observed.get(name) != value]
    for name, value in EXPECTED.items():
        print(f'{name:<18} {observed.get(name, "missing"):<8} {"ok" if name not in failed else "FAILED, expected " + value}')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
