#!/usr/bin/env python3
"""Check passkeys through the real engine.

A page on an HTTPS site registers a passkey with `navigator.credentials.create`
and signs in with `navigator.credentials.get`, twice. The browser's
authenticator answers, and the page itself verifies each answer with WebCrypto:
the client data, the authenticator data and the signature under the public key
it was given. A relying party ID of another site must be refused with a
SecurityError, a declined request with a NotAllowedError, and an aborted one
must tell the browser.

It needs a benchmark build (see docs/development.md) and network access to
example.com.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    arguments = parser.parse_args()
    metadata = json.loads((ROOT / 'DEPENDENCIES.json').read_text())
    native = metadata['native_runtime']
    with tempfile.TemporaryDirectory(prefix='enzim-passkey-check-') as scratch:
        environment = os.environ.copy()
        for name, part in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache')]:
            path = Path(scratch) / part
            path.mkdir(mode=0o700)
            environment[name] = str(path)
        result = subprocess.run([
            'systemd-run', '--user', '--scope', '--quiet', '-p', 'MemoryMax=3G', '-p', 'MemorySwapMax=0', '--',
            sys.executable, str(ROOT / metadata['webkit']['path'] / 'tools/source_runtime.py'),
            '--directory', str(ROOT / native['path']), '--source-pin', str(ROOT / native['source_pin']),
            'run', '--', str(arguments.enzim_binary.resolve()), '--passkeys-check',
        ], env=environment, capture_output=True, text=True, check=False)
    line = next((line for line in result.stderr.splitlines() if line.startswith('PASSKEYS_CHECK')), '')
    print(line or result.stderr[-2000:])
    return 0 if result.returncode == 0 and line.startswith('PASSKEYS_CHECK {') else 1


if __name__ == '__main__':
    sys.exit(main())
