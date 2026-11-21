#!/usr/bin/env python3
"""Check that a preloaded script is requested after the style sheet of its HTTP/2 document.

YouTube's home page names its 2 MiB script with `<link rel=preload>` at the top
of its head and its style sheet much further down. An HTTP/2 server delivers
whatever is requested moments after such a script behind the megabytes it has
already written, so the engine holds the script's request while the document is
arriving. This loads the page twice, with the hold and with
`WPE_SCRIPT_PRELOAD_HOLD_MS=0`, and reads the order of the two requests on the
document's connection from libsoup's HTTP/2 log.

With the hold the style sheet must be requested first. Without it the script
must be requested first: a page that no longer shows the old order would let a
broken hold pass, and is reported as such instead.

This is a check against a live site. It needs a benchmark build (see
docs/development.md) and the graphical session of `tools/benchmark_sites.py`,
which it shares.
"""
import argparse
from pathlib import Path
import re
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)

PAGE = 'https://www.youtube.com/'
REQUEST = re.compile(r'libsoup-http2-DEBUG: \S+ \[CLIENT\] \[C(\d+)-S(\d+)\] \[\w+\] \[SESSION\] Request made for (\S+)')


def request_order(log):
    """The positions of the style sheet's and the script's request after the last request for the document.

    The page reloads itself once to apply the colour scheme; the last document
    is the one that is shown.
    """
    requests = [match.groups() for match in map(REQUEST.search, log.read_text(errors='replace').splitlines()) if match]
    documents = [index for index, (_, _, path) in enumerate(requests)
                 if path.split('?')[0] == 'www.youtube.com/']
    if not documents:
        return None
    connection = requests[documents[-1]][0]
    later = [path for other, _, path in requests[documents[-1] + 1:] if other == connection]
    sheet = next((index for index, path in enumerate(later) if '/_/ss/' in path), None)
    script = next((index for index, path in enumerate(later) if '/_/js/' in path), None)
    return sheet, script


def load(arguments, environment, extra):
    with tempfile.TemporaryDirectory(prefix='script-preload-order-') as scratch:
        options = argparse.Namespace(runtime=arguments.runtime, source_pin=arguments.source_pin, enzim_binary=arguments.enzim_binary,
                                     env=[*arguments.env, 'G_MESSAGES_DEBUG=libsoup-http2', *extra], resources=False,
                                     memory_max='6G')
        browser = benchmark_sites.Enzim(options, Path(scratch), environment)
        try:
            browser.start()
            time.sleep(3)
            requested = browser.open(PAGE)
            deadline = time.monotonic() + 30
            loaded = False
            while not loaded and time.monotonic() < deadline:
                time.sleep(0.2)
                current = browser.poll()
                loaded = bool(current and current.get('origin', 0) / 1000 >= requested - 1 and current.get('load'))
            time.sleep(1)
            return loaded, request_order(browser.log)
        finally:
            browser.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    parser.add_argument('--runtime', default=str(benchmark_sites.ROOT / benchmark_sites.DEPENDENCIES['native_runtime']['path']))
    parser.add_argument('--source-pin', type=Path)
    parser.add_argument('--env', action='append', default=[], metavar='K=V')
    arguments = parser.parse_args()

    environment = benchmark_sites.desk()
    held_loaded, held = load(arguments, environment, [])
    control_loaded, control = load(arguments, environment, ['WPE_SCRIPT_PRELOAD_HOLD_MS=0'])

    def both(order):
        return order is not None and None not in order
    # Without the old order in the control, the page has changed and the first result says nothing.
    conclusive = control_loaded and both(control) and control[1] < control[0]
    checks = {
        'the page loads with the hold and without it': held_loaded and control_loaded,
        'without the hold the script is requested before the style sheet': conclusive,
        'with the hold the style sheet is requested before the script': both(held) and held[0] < held[1],
    }
    for name, passed in checks.items():
        print(f'{"ok    " if passed else "FAILED"} {name}')
    print(f'request positions after the document (style sheet, script): held {held}, control {control}')
    if not conclusive:
        print('The control did not show the old order: the page may have changed, and this check then proves nothing.')
        return 2
    return 0 if all(checks.values()) else 1


if __name__ == '__main__':
    sys.exit(main())
