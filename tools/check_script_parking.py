#!/usr/bin/env python3
"""Check that a hidden tab's scripts still compile and print correctly after their text was parked.

A web process that shows no page keeps a deflated copy of script text it is not
reading, and inflates it when a function is next compiled or printed. This
loads a page with a 300 KB ASCII script and a 60 KB non-ASCII one, neither of
them cacheable so that both take the deflating path, hides the tab until its
scripts are parked, shows it, and repeats. On each return the page runs every
function, half of them for the first time, and prints all of them; the sums
must equal the ones computed here and the printed text must not have changed.
The engine's trace must show the scripts parked and read back, so that a run in
which nothing was parked does not pass.

It needs a benchmark build (see docs/development.md), `wtype`, and the
graphical session of `tools/benchmark_sites.py`, which it shares.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import os
import re
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)

PLAIN_COUNT, ACCENTED_COUNT = 4000, 600
ACCENTED_TEXT = 'café-日本-'


def plain_script():
    """f<i>(a, b) returns a * 3 + b + i plus the length of a string built from i and b."""
    parts = [f"function f{i}(a,b){{var s='n{i}-'+b;var t=[a,b,{i}];if(t.length>2){{s=s.toUpperCase()+t.join('-');}}"
             f"return a*3+b+{i}+s.length;}}\n" for i in range(PLAIN_COUNT)]
    parts.append('var PLAIN=[' + ','.join(f'f{i}' for i in range(PLAIN_COUNT)) + '];\n')
    return ''.join(parts)


def plain_value(i, a, b):
    return a * 3 + b + i + len(f'n{i}-{b}') + len(f'{a}-{b}-{i}')


def accented_script():
    parts = [f"function u{i}(a){{var s='{ACCENTED_TEXT}{i}-'+a;return s.length+s.charCodeAt(3)+s.charCodeAt(5)+{i};}}\n"
             for i in range(ACCENTED_COUNT)]
    parts.append('var ACCENTED=[' + ','.join(f'u{i}' for i in range(ACCENTED_COUNT)) + '];\n')
    return ''.join(parts)


def accented_value(i, a):
    text = f'{ACCENTED_TEXT}{i}-{a}'
    return len(text) + ord(text[3]) + ord(text[5]) + i


PAGE = '''<!doctype html><meta charset=utf-8><title>parking loading</title><body><h1>script parking check</h1><p id=out></p>
<script src="/plain.js"></script>
<script src="/accented.js" charset="utf-8"></script>
<script>
function hash(s){var h=2166136261;for(var i=0;i<s.length;i++){h^=s.charCodeAt(i);h=Math.imul(h,16777619);}return h>>>0;}
function text(list){var h=0;for(var i=0;i<list.length;i++)h=(h*31+hash(list[i].toString()))>>>0;return h;}
function plain(from,to){var s=0;for(var i=from;i<to;i++)s+=PLAIN[i](i,3);return s;}
function accented(from,to){var s=0;for(var i=from;i<to;i++)s+=ACCENTED[i](i);return s;}
var half=PLAIN.length/2,accentedHalf=ACCENTED.length/2;
// Half of each script's functions run now; the rest are first compiled after a return.
var before={first:plain(0,half),accentedFirst:accented(0,accentedHalf),text:text(PLAIN),accentedText:text(ACCENTED)};
document.title='parking ready';
// One report per load: a page loaded again after its process died must not pass for one that returned.
new Image().src='/report?loaded='+Date.now();
function report(){
  var same=plain(0,half)===before.first&&accented(0,accentedHalf)===before.accentedFirst
    &&text(PLAIN)===before.text&&text(ACCENTED)===before.accentedText;
  var rest=plain(half,PLAIN.length),accentedRest=accented(accentedHalf,ACCENTED.length);
  document.getElementById('out').textContent='returned '+(same?'same':'different');
  new Image().src='/report?returned='+(same?'same':'different')+'&rest='+rest+'&accented='+accentedRest+'&at='+Date.now();
}
// Window capture runs before listeners on the document.
window.addEventListener('visibilitychange',function(){
  if(document.visibilityState!=='visible')return;
  try{report();}catch(e){new Image().src='/report?error='+encodeURIComponent(String(e))+'&at='+Date.now();}
},true);
</script>'''
OTHER = '<!doctype html><meta charset=utf-8><title>parking other tab</title><body><h1>other tab</h1>'


class Handler(BaseHTTPRequestHandler):
    reports = []
    bodies = {}

    def log_message(self, *_):
        pass

    def do_GET(self):
        url = urlparse(self.path)
        if url.path == '/report':
            self.reports.append({key: values[0] for key, values in parse_qs(url.query).items()})
        kind, body = self.bodies.get(url.path, ('text/plain', b''))
        self.send_response(200 if body else 404)
        self.send_header('Content-Type', kind)
        # Not stored, so that the engine holds the bytes itself instead of a mapping of its disk cache.
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    parser.add_argument('--runtime', default=str(benchmark_sites.ROOT / benchmark_sites.DEPENDENCIES['native_runtime']['path']))
    parser.add_argument('--source-pin', type=Path)
    parser.add_argument('--env', action='append', default=[], metavar='K=V')
    parser.add_argument('--hidden', type=float, default=25, help='Seconds a tab stays hidden; the trim comes after eight')
    parser.add_argument('--returns', type=int, default=2)
    arguments = parser.parse_args()

    Handler.bodies = {'/': ('text/html; charset=utf-8', PAGE.encode()), '/other': ('text/html; charset=utf-8', OTHER.encode()),
                      '/plain.js': ('text/javascript', plain_script().encode()),
                      '/accented.js': ('text/javascript; charset=utf-8', accented_script().encode())}
    plain_size = len(Handler.bodies['/plain.js'][1]) // 1024
    expected_rest = sum(plain_value(i, i, 3) for i in range(PLAIN_COUNT // 2, PLAIN_COUNT))
    expected_accented = sum(accented_value(i, i) for i in range(ACCENTED_COUNT // 2, ACCENTED_COUNT))
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    address = f'http://127.0.0.1:{server.server_address[1]}'

    environment = benchmark_sites.desk()
    session = {**os.environ, **environment}
    session.pop('DISPLAY', None)
    returned = lambda: [r for r in Handler.reports if 'returned' in r or 'error' in r]  # noqa: E731

    def bring_to_front(browser, title):
        # A keystroke is sometimes not taken after a long idle, so the window title decides. A tab
        # that is still loading has no title of its own yet and must not be taken for the other one.
        deadline = time.monotonic() + 5
        while 'parking' not in browser.title() and time.monotonic() < deadline:
            time.sleep(0.25)
        for _ in range(6):
            if title in browser.title():
                return
            subprocess.run(['wtype', '-M', 'ctrl', '-M', 'shift', '-k', 'Tab', '-m', 'shift', '-m', 'ctrl'], env=session, check=True)
            time.sleep(1)
        raise RuntimeError(f'Could not bring the "{title}" tab to the front')

    with tempfile.TemporaryDirectory(prefix='script-parking-') as scratch:
        options = argparse.Namespace(runtime=arguments.runtime, source_pin=arguments.source_pin, enzim_binary=arguments.enzim_binary,
                                     env=arguments.env + ['WPE_HIDDEN_PROCESS_TRIM_TRACE=1'], resources=False, memory_max='6G')
        browser = benchmark_sites.Enzim(options, Path(scratch), environment)
        try:
            browser.start()
            time.sleep(3)
            browser.open(address + '/')
            time.sleep(4)
            browser.open(address + '/other')
            for _ in range(arguments.returns):
                bring_to_front(browser, 'parking other tab')
                time.sleep(arguments.hidden)
                seen = len(returned())
                bring_to_front(browser, 'parking ready')
                deadline = time.monotonic() + 10
                while len(returned()) == seen and time.monotonic() < deadline:
                    time.sleep(0.25)
        finally:
            browser.stop()
        log = (Path(scratch) / 'browser.log').read_text(errors='replace')
    server.shutdown()

    parked = [int(size) for size in re.findall(r'WPE_SCRIPT_PARKING pass candidates=\d+ parked=\d+ bytesK=(\d+)', log)]
    read = [int(size) for size in re.findall(r'WPE_SCRIPT_PARKING read bytesK=(\d+)', log)]
    loads = sum('loaded' in report for report in Handler.reports)
    reports = returned()
    checks = {
        'the page loaded once': loads == 1,
        f'it reported {arguments.returns} returns': len(reports) == arguments.returns,
        'no return raised a script error': not any('error' in report for report in reports),
        'functions that had run return and print as before': bool(reports) and all(r.get('returned') == 'same' for r in reports),
        'functions first compiled after a return are correct': bool(reports) and all(
            r.get('rest') == str(expected_rest) and r.get('accented') == str(expected_accented) for r in reports),
        'the trace shows the large script parked': any(size >= plain_size for size in parked),
        'the trace shows it read back': any(size >= plain_size - 1 for size in read),
    }
    for name, passed in checks.items():
        print(f'{"ok    " if passed else "FAILED"} {name}')
    if not all(checks.values()):
        print('reports:', Handler.reports)
        print('parked (KiB):', parked, 'read back (KiB):', read)
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
