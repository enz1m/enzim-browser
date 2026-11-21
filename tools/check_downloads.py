#!/usr/bin/env python3
"""Qualify native downloads with isolated storage and an authenticated HTTP fixture."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path)
parser.add_argument('--receipts', type=Path, default=ROOT / 'work/downloads/native-receipts.json')
args = parser.parse_args()
if args.binary is None:
    target = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--offline', '--locked', '--no-deps', '--format-version=1'],
        cwd=ROOT))['target_directory']
    args.binary = Path(target) / 'debug/enzim-browser'
if not args.binary.is_file():
    parser.error('Build with ./tools/build --features benchmark first')
args.receipts.parent.mkdir(parents=True, exist_ok=True)
metadata = json.loads((ROOT / 'DEPENDENCIES.json').read_text())
runtime = (ROOT / metadata['native_runtime']['path']).resolve()
runtime_arguments = ['--directory', str(runtime)]
if metadata['native_runtime'].get('source_pin'):
    pin = ROOT / metadata['native_runtime']['source_pin']
    if hashlib.sha256(pin.read_bytes()).hexdigest() != metadata['native_runtime']['source_pin_sha256']:
        parser.error('Reviewed browser source pin changed')
    runtime_arguments += ['--source-pin', str(pin)]
fixture = subprocess.Popen(
    ['python3', str(ROOT / 'tools/fixtures/downloads.py'), '--receipts', str(args.receipts.resolve())],
    stdout=subprocess.PIPE, text=True,
)
try:
    line = fixture.stdout.readline().strip()
    if not line.startswith('ENZIM_DOWNLOAD_BASE='):
        raise RuntimeError('The download fixture did not start')
    with tempfile.TemporaryDirectory(prefix='enzim-download-check-') as scratch:
        env = os.environ.copy()
        env['ENZIM_DOWNLOAD_BASE'] = line.split('=', 1)[1]
        env['ENZUI_PROBE_TRUSTED_LOCAL'] = '1'
        for name, part in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache')]:
            path = Path(scratch) / part
            path.mkdir(mode=0o700)
            env[name] = str(path)
        command = [
            'systemd-run', '--user', '--scope', '-p', 'MemoryMax=3G', '-p', 'MemorySwapMax=0', '--',
            'python3', str(ROOT / metadata['webkit']['path'] / 'tools/source_runtime.py'),
            *runtime_arguments, 'run', '--', str(args.binary.resolve()), '--downloads-check',
        ]
        result = subprocess.run(command, env=env, check=False)
finally:
    fixture.terminate()
    fixture.wait()
raise SystemExit(result.returncode)
