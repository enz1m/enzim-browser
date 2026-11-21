#!/usr/bin/env python3
"""Check that custom properties declared on every element still resolve per element.

Sites such as The New York Times declare a block of custom properties with
`:root, :where(:root *)`, which every element matches. The engine remembers
when resolving such a block gives back the parent's values and then skips it
(`WPE_PARENT_CUSTOM_PROPERTIES_CACHE=0` turns that off). This page declares a
block like that, fills the document so that the block is remembered, and then
reads computed values where the answer must differ from the parent's: under an
element that overrides a value, with attr(), after a property is registered
(it is then computed against each element's font size) and after a change at
the root. The page reports in its title.

A runtime whose remembered blocks ignore the parent fails it with
`s2 --c="7px 7px end"`.

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

PAGE = r'''<!doctype html><meta charset=utf-8><title>running</title>
<style>
:root, :where(:root *) { --a: 1px; --b: var(--a) var(--a); --c: var(--b) end; --u: 10em; }
.override { --a: 7px; }
.t { --t: attr(data-x); }
.deep * { color: blue; }
</style>
<body>
<div id=plain><p><span id=s1>x</span></p></div>
<div class=override id=o><p><span id=s2>x</span></p></div>
<div class=deep><div><div><div><div><span id=s3>x</span></div></div></div></div></div>
<p class=t data-x="one" id=t1></p><p class=t data-x="two" id=t2></p>
<div style="font-size: 20px"><span id=u1>x</span></div>
<script>
const failures = [];
const value = (id, name) => getComputedStyle(document.getElementById(id)).getPropertyValue(name).trim();
function expect(id, name, wanted) {
  const got = value(id, name);
  if (got !== wanted) failures.push(`${id} ${name}=${JSON.stringify(got)} want ${JSON.stringify(wanted)}`);
}
const filler = document.createElement('div');
for (let i = 0; i < 300; i++) filler.appendChild(document.createElement('span'));
document.body.appendChild(filler);
document.body.offsetTop;
expect('s1', '--c', '1px 1px end');
expect('o', '--c', '7px 7px end');
// Below the override every element declares --a again, so the parent's 7px must not survive.
expect('s2', '--c', '1px 1px end');
expect('s3', '--c', '1px 1px end');
expect('t1', '--t', '"one"');
expect('t2', '--t', '"two"');
expect('u1', '--u', '10em');
CSS.registerProperty({ name: '--u', syntax: '<length>', inherits: true, initialValue: '0px' });
document.body.offsetTop;
expect('u1', '--u', '200px');
document.documentElement.style.setProperty('--a', '3px');
document.body.offsetTop;
expect('s1', '--c', '1px 1px end');
const root = getComputedStyle(document.documentElement).getPropertyValue('--c').trim();
if (root !== '3px 3px end') failures.push(`root --c=${root}`);
document.title = failures.length ? 'FAIL ' + failures.join('; ') : 'PASS';
</script>
'''


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        body = PAGE.encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def page_title(arguments, environment, port):
    with tempfile.TemporaryDirectory(prefix='custom-property-reuse-') as scratch:
        options = argparse.Namespace(runtime=arguments.runtime, source_pin=arguments.source_pin, enzim_binary=arguments.enzim_binary,
                                     env=arguments.env, resources=False, memory_max='6G')
        browser = benchmark_sites.Enzim(options, Path(scratch), environment)
        try:
            browser.start()
            time.sleep(3)
            browser.open(f'http://127.0.0.1:{port}/')
            title = ''
            # The window title reaches the compositor a little after the page sets it.
            for _ in range(60):
                time.sleep(0.5)
                title = browser.title()
                if title.startswith(('PASS', 'FAIL')):
                    break
            return title
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
    title = page_title(arguments, benchmark_sites.desk(), server.server_address[1])
    server.shutdown()

    passed = title == 'PASS'
    print(f'{"ok    " if passed else "FAILED"} custom properties declared on every element resolve per element')
    if not passed:
        print('page title:', title or '(none)')
    return 0 if passed else 1


if __name__ == '__main__':
    sys.exit(main())
