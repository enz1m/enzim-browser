#!/usr/bin/env python3
"""Build native rule-only cache entries with the manifest-pinned WPE runtime."""
import argparse
import configparser
import ctypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]


def compile_filters(output, runtime):
    webkit = ctypes.CDLL(str(runtime / 'lib/libWPEWebKit-2.0.so.1'))
    glib = ctypes.CDLL('libglib-2.0.so.0')
    gobject = ctypes.CDLL('libgobject-2.0.so.0')
    pointer = ctypes.c_void_p
    callback_type = ctypes.CFUNCTYPE(None, pointer, pointer, pointer)
    webkit.webkit_user_content_filter_store_new.argtypes = [ctypes.c_char_p]
    webkit.webkit_user_content_filter_store_new.restype = pointer
    webkit.webkit_user_content_filter_store_save.argtypes = [pointer, ctypes.c_char_p, pointer, pointer, callback_type, pointer]
    webkit.webkit_user_content_filter_store_save_finish.argtypes = [pointer, pointer, ctypes.POINTER(pointer)]
    webkit.webkit_user_content_filter_store_save_finish.restype = pointer
    webkit.webkit_user_content_filter_get_identifier.argtypes = [pointer]
    webkit.webkit_user_content_filter_get_identifier.restype = ctypes.c_char_p
    webkit.webkit_user_content_filter_unref.argtypes = [pointer]
    glib.g_bytes_new.argtypes = [pointer, ctypes.c_size_t]
    glib.g_bytes_new.restype = pointer
    glib.g_bytes_unref.argtypes = [pointer]
    glib.g_error_free.argtypes = [pointer]
    glib.g_main_context_iteration.argtypes = [pointer, ctypes.c_int]
    gobject.g_object_unref.argtypes = [pointer]
    native = output / 'native'
    native.mkdir(exist_ok=True, mode=0o700)
    cache = native / 'store'
    cache.mkdir(exist_ok=True, mode=0o700)
    store = webkit.webkit_user_content_filter_store_new(os.fsencode(cache))
    if not store:
        raise RuntimeError('Native rule-cache construction failed')
    try:
        for mode in [5, 6, 9, 10]:
            source = (output / f'filters-{mode}.json').read_bytes()
            if len(source) > 16 * 1024 * 1024 or b'\0' in source:
                raise ValueError('Native filter source exceeds SDK bounds')
            identity = ('enzui-rules-v1-' + hashlib.sha256(source).hexdigest()).encode()
            completed = []

            @callback_type
            def finish(owner, result, _):
                error = pointer()
                compiled = webkit.webkit_user_content_filter_store_save_finish(owner, result, ctypes.byref(error))
                if compiled:
                    completed.append(webkit.webkit_user_content_filter_get_identifier(compiled) == identity)
                    webkit.webkit_user_content_filter_unref(compiled)
                else:
                    completed.append(False)
                if error.value:
                    glib.g_error_free(error)

            # GBytes copies source; the callback stays live until native completion.
            data = glib.g_bytes_new(source, len(source))
            if not data:
                raise MemoryError('Native filter source allocation failed')
            webkit.webkit_user_content_filter_store_save(store, identity, data, None, finish, None)
            glib.g_bytes_unref(data)
            deadline = time.monotonic() + 60
            while not completed:
                if time.monotonic() >= deadline:
                    raise TimeoutError(f'Native filter {mode} did not finish')
                glib.g_main_context_iteration(None, False)
                time.sleep(.001)
            if completed != [True]:
                raise RuntimeError(f'Native filter {mode} compilation failed')
            shutil.copyfile(cache / ('ContentRuleList-' + identity.decode()), native / f'{mode}.bin')
    finally:
        gobject.g_object_unref(store)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--native', action='store_true')
    args = parser.parse_args()
    metadata = json.loads((ROOT / 'DEPENDENCIES.json').read_text())
    if os.environ.get('ENZIM_BROWSER_FLATPAK_BUILD') == '1':
        runtime = Path('/usr/local')
        version = configparser.ConfigParser(interpolation=None)
        with (runtime / 'share/enzim-webkit/runtime.ini').open() as descriptor:
            version.read_file(descriptor)
        if version['Runtime']['SourceCommit'] != metadata['native_runtime']['source_commit']:
            raise ValueError('The Flatpak SDK does not match the Browser engine source pin')
        compile_filters(args.output.resolve(), runtime)
        return
    runtime = (ROOT / metadata['native_runtime']['path']).resolve()
    if args.native:
        compile_filters(args.output.resolve(), runtime)
    else:
        launcher = ROOT / metadata['webkit']['path'] / 'tools/source_runtime.py'
        runtime_arguments = ['--directory', str(runtime)]
        if metadata['native_runtime'].get('source_pin'):
            pin = ROOT / metadata['native_runtime']['source_pin']
            if hashlib.sha256(pin.read_bytes()).hexdigest() != metadata['native_runtime']['source_pin_sha256']:
                raise ValueError('Reviewed browser source pin changed')
            runtime_arguments += ['--source-pin', str(pin)]
        subprocess.run([sys.executable, str(launcher), *runtime_arguments, 'run', '--',
                        sys.executable, str(Path(__file__).resolve()), str(args.output.resolve()), '--native'], check=True)


if __name__ == '__main__':
    main()
