#!/usr/bin/env python3
"""Prepare the pinned private Bitwarden helper and its distribution notices."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PIN = json.loads((ROOT / "assets/connectors/bitwarden.json").read_text())


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(128 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def verify(path, size, expected):
    if path.is_symlink() or not path.is_file() or path.stat().st_size != size:
        raise ValueError(f"Missing or invalid bundled file: {path.name}")
    if digest(path) != expected:
        raise ValueError(f"Bundled file identity changed: {path.name}")


def verify_bundle(directory):
    verify(directory / "bw", PIN["binary_size"], PIN["binary_sha256"])
    verify(directory / "source.tar.gz", PIN["source_size"], PIN["source_sha256"])
    for name in ("LICENSE.txt", "LICENSE_GPL.txt", "NOTICE.md"):
        if (directory / name).read_bytes() != (ROOT / "assets/connectors/bitwarden" / name).read_bytes():
            raise ValueError(f"Bundle notice changed: {name}")


def acquire(supplied, url, size, expected, cache, offline):
    path = supplied or cache / (expected + ".download")
    if not path.exists():
        if offline or supplied:
            raise ValueError("Bundle source missing; run tools/prepare_bitwarden.py before an offline build.")
        cache.mkdir(parents=True, exist_ok=True, mode=0o700)
        with tempfile.NamedTemporaryFile(dir=cache, delete=False) as temporary:
            staging = Path(temporary.name)
            try:
                with urllib.request.urlopen(url, timeout=30) as response:
                    remaining = size
                    while block := response.read(min(128 * 1024, remaining + 1)):
                        remaining -= len(block)
                        if remaining < 0:
                            raise ValueError("Bundle download exceeds its pinned size.")
                        temporary.write(block)
                temporary.flush()
                verify(staging, size, expected)
                os.replace(staging, path)
            finally:
                staging.unlink(missing_ok=True)
    verify(path, size, expected)
    return path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path)
    parser.add_argument("--source-archive", type=Path)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--install-prefix", type=Path,
                        help="Copy the verified helper, source and notices into PREFIX/libexec/enzim-browser/bitwarden")
    args = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() not in ("x86_64", "AMD64"):
        parser.error("This pinned helper package supports Linux x86_64.")
    directory = (ROOT / PIN["development_directory"]).resolve()
    try:
        if not directory.exists() and not args.check:
            cache = ROOT / json.loads((ROOT / "DEPENDENCIES.json").read_text())["artifacts_directory"] / "connector-downloads"
            archive = acquire(args.archive, PIN["asset_url"], PIN["asset_size"],
                              PIN["asset_sha256"], cache, args.offline)
            source = acquire(args.source_archive, PIN["source_url"], PIN["source_size"],
                             PIN["source_sha256"], cache, args.offline)
            directory.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            with tempfile.TemporaryDirectory(dir=directory.parent) as temporary:
                staging = Path(temporary) / "bundle"
                staging.mkdir(mode=0o700)
                with zipfile.ZipFile(archive) as package:
                    member = package.getinfo("bw")
                    if member.file_size != PIN["binary_size"]:
                        raise ValueError("Unexpected helper executable size.")
                    with package.open(member) as stream, (staging / "bw").open("xb") as output:
                        shutil.copyfileobj(stream, output, 128 * 1024)
                (staging / "bw").chmod(0o755)
                shutil.copyfile(source, staging / "source.tar.gz")
                for name in ("LICENSE.txt", "LICENSE_GPL.txt", "NOTICE.md"):
                    shutil.copyfile(ROOT / "assets/connectors/bitwarden" / name, staging / name)
                verify_bundle(staging)
                (staging / "bundle.json").write_text(json.dumps(PIN, indent=2) + "\n")
                clean = {"PATH": "/usr/bin:/bin", "HOME": temporary}
                version = subprocess.run([str(staging / "bw"), "--version"], env=clean,
                                         capture_output=True, text=True, timeout=15, check=True)
                if version.stdout.strip() != PIN["version"]:
                    raise ValueError("Unexpected helper version.")
                staging.rename(directory)
        verify_bundle(directory)
        if args.install_prefix:
            target = args.install_prefix / "libexec/enzim-browser/bitwarden"
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(directory, target)
            target.chmod(0o755)
            verify_bundle(target)
        print(f"Verified bundled Bitwarden OSS helper {PIN['version']}")
    except (OSError, ValueError, subprocess.SubprocessError, zipfile.BadZipFile) as error:
        parser.exit(1, str(error) + "\n")


if __name__ == "__main__":
    main()
