#!/usr/bin/env python3
"""Install this source checkout's runtime-aware desktop launcher and MIME entry."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def install(binary, data_home, bin_dir, refresh=True):
    binary = binary.resolve(strict=True)
    applications = data_home / 'applications'
    icon_dir = data_home / 'icons/hicolor/scalable/apps'
    for directory in (applications, icon_dir, bin_dir):
        directory.mkdir(parents=True, exist_ok=True)
    launcher = bin_dir / 'enzim-browser-launch'
    source = ROOT / 'tools/browser'
    temporary = bin_dir / '.enzim-browser-launch.new'
    temporary.unlink(missing_ok=True)
    temporary.write_text(
        '#!' + sys.executable + '\nimport os, sys\n'
        + 'os.execv(' + repr(sys.executable) + ', [' + repr(sys.executable)
        + ', ' + repr(str(source)) + ', "--binary", ' + repr(str(binary))
        + ', *sys.argv[1:]])\n'
    )
    temporary.chmod(0o755)
    temporary.replace(launcher)
    entry = (ROOT / 'resources/org.enzim.Browser.desktop').read_text()
    escaped = str(launcher).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$')
    entry = entry.replace('Exec=enzim-browser-launch %U', f'Exec="{escaped}" %U')
    entry = entry.replace('TryExec=enzim-browser-launch', 'TryExec=' + str(launcher))
    desktop = applications / 'org.enzim.Browser.desktop'
    desktop.write_text(entry)
    icon = icon_dir / 'org.enzim.Browser.svg'
    if not icon.exists():
        shutil.copyfile(ROOT / 'resources/org.enzim.Browser.svg', icon)
    validator = shutil.which('desktop-file-validate')
    if validator:
        subprocess.run([validator, str(desktop)], check=True)
    if refresh:
        subprocess.run(['update-desktop-database', str(applications)], check=True)
        cache = shutil.which('gtk-update-icon-cache')
        if cache:
            subprocess.run([cache, '-f', '-t', str(data_home / 'icons/hicolor')], check=True)
    return desktop


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--data-home', type=Path, default=Path(os.environ.get('XDG_DATA_HOME', Path.home() / '.local/share')))
    parser.add_argument('--bin-dir', type=Path, default=Path.home() / '.local/bin')
    args = parser.parse_args()
    binary = args.binary
    if binary is None:
        metadata = json_metadata()
        binary = Path(metadata['target_directory']) / 'debug/enzim-browser'
    print(install(binary, args.data_home.expanduser().resolve(), args.bin_dir.expanduser().resolve()))


def json_metadata():
    import json
    return json.loads(subprocess.check_output(['cargo', 'metadata', '--offline', '--locked', '--no-deps', '--format-version=1'], cwd=ROOT))


if __name__ == '__main__':
    main()
