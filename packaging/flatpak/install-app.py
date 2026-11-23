#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Install one Cargo executable and its exact credential guard into /app."""
import argparse
import json
from pathlib import Path
import re
import shlex
import shutil

GUARD = 'libenzui_credential_guard.so'


def artifacts(messages, target):
    binaries, guards = set(), set()
    completed = False
    for line in messages.splitlines():
        try:
            item = json.loads(line)
        except json.JSONDecodeError:
            continue
        if item.get('reason') == 'build-finished':
            if not item.get('success'):
                raise ValueError('Cargo build failed; refusing stale artifacts')
            completed = True
        if item.get('reason') == 'compiler-artifact' and item.get('target', {}).get('name') == target and item.get('executable'):
            binaries.add(Path(item['executable']))
        if item.get('reason') == 'build-script-executed':
            guards.update(Path(value) / GUARD for name, value in item.get('env', [])
                          if name == 'ENZUI_WPE_EXTENSION_DIR')
    if not completed:
        raise ValueError('Cargo JSON does not contain a successful completed build')
    if len(binaries) != 1 or len(guards) != 1:
        raise ValueError('expected exactly one executable and one enzui-webkit guard in Cargo JSON; build only the selected app')
    binary, guard = binaries.pop(), guards.pop()
    if not binary.is_file() or not guard.is_file():
        raise ValueError('Cargo executable or credential guard is missing')
    return binary, guard


def install(messages, target, prefix):
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]*', target):
        raise ValueError('invalid executable name')
    binary, guard = artifacts(messages, target)
    for directory in ['bin', 'libexec/enzim-browser', 'lib/wpe-extensions']:
        (prefix / directory).mkdir(parents=True, exist_ok=True)
    shutil.copy2(binary, prefix / 'libexec/enzim-browser' / target)
    shutil.copy2(guard, prefix / 'lib/wpe-extensions' / GUARD)
    wrapper = prefix / 'bin' / target
    wrapper.write_text('#!/bin/sh\nexec /usr/local/bin/enzim-webkit-run --abi=1 -- ' + shlex.quote('/app/libexec/enzim-browser/' + target) + ' "$@"\n')
    wrapper.chmod(0o755)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cargo-messages', type=Path, required=True)
    parser.add_argument('--target', required=True)
    parser.add_argument('--prefix', type=Path, default=Path('/app'))
    args = parser.parse_args()
    try:
        install(args.cargo_messages.read_text(), args.target, args.prefix)
    except (ValueError, OSError) as error:
        parser.exit(1, str(error) + '\n')


if __name__ == '__main__':
    main()
