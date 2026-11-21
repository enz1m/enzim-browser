#!/usr/bin/env python3
"""Restore pinned fs-verity identities on a private native-runtime copy."""
import argparse
import errno
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def seal_file(helper, path, expected):
    helper.verify_file(path, expected)
    seal = expected.get("fsverity")
    if seal is None:
        return "portable-sha256"
    measured = helper.measure_verity(path)
    if measured is not None:
        if measured != seal:
            raise ValueError(f"fs-verity identity changed: {path.name}")
        return "already-sealed"
    if path.stat().st_nlink != 1:
        raise ValueError(f"copy the multiply-linked file before sealing: {path.name}")
    try:
        measured = helper.enable_verity(path)
    except OSError as error:
        if error.errno not in (errno.EOPNOTSUPP, errno.ENOTTY, errno.EINVAL,
                               errno.EPERM, errno.EACCES, errno.EBUSY):
            raise
        return "portable-sha256"
    if measured != seal:
        raise ValueError(f"fs-verity identity changed: {path.name}")
    return "sealed"


def main():
    metadata = json.loads((ROOT / "DEPENDENCIES.json").read_text())
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runtime", nargs="?", type=Path,
                        default=ROOT / metadata["native_runtime"]["path"])
    args = parser.parse_args()
    directory = args.runtime.resolve()
    if not directory.is_relative_to((ROOT / metadata["artifacts_directory"]).resolve()):
        parser.error("only private copies in enz_browser-deps may be sealed")
    source = (ROOT / metadata["webkit"]["path"] / "tools/source_runtime.py").resolve()
    spec = importlib.util.spec_from_file_location("enzim_source_runtime", source)
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    source_pin = ROOT / metadata["native_runtime"]["source_pin"] if metadata["native_runtime"].get("source_pin") else None
    manifest = helper.verify(directory, source_pin)
    results = {}
    for name, expected in manifest["files"].items():
        if "link" not in expected:
            results[name] = seal_file(helper, directory / name, expected)
    helper.verify(directory, source_pin)
    print(json.dumps({"runtime": str(directory), "files": results}, indent=2))


if __name__ == "__main__":
    main()
