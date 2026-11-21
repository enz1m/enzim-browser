import json
import runpy
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


class DesktopLaunchTests(unittest.TestCase):
    def test_open_with_passes_multiple_files_as_uris_and_keeps_profile_flags(self):
        with tempfile.TemporaryDirectory(prefix='enzim-desktop-launch-') as scratch:
            root = Path(scratch)
            runtime = root / 'runtime'
            runtime.mkdir()
            (runtime / 'source-runtime.json').write_text(json.dumps({'source': {'source_commit': 'fixture'}}))
            first = root / 'report #é.html'
            first.write_text('<h1>report</h1>')
            missing = root / 'missing private name.pdf'
            second = (root / 'second.pdf').as_uri()
            argv = ['browser', str(first), '--binary', '/bin/true', '--runtime', str(runtime), '--profile', 'Business', second, str(missing), 'https://example.test/?q=one&two=2']
            with patch('sys.argv', argv), patch('os.execvp') as execute:
                runpy.run_path(str(ROOT / 'tools/browser'), run_name='__main__')
            command = execute.call_args.args[1]
            inputs = command[command.index('--browser') + 1:command.index('--profile')]
            self.assertEqual(inputs, [first.as_uri(), second, missing.as_uri(), 'https://example.test/?q=one&two=2'])
            self.assertEqual(command[-2:], ['--profile', 'Business'])

    def test_source_install_is_runtime_aware_and_does_not_create_defaults(self):
        module = runpy.run_path(str(ROOT / 'tools/install_desktop.py'))
        with tempfile.TemporaryDirectory(prefix='enzim-desktop-install-') as scratch:
            root = Path(scratch)
            desktop = module['install'](Path('/bin/true'), root / 'data', root / 'bin', refresh=False)
            text = desktop.read_text()
            self.assertIn(' %U\n', text)
            self.assertIn('application/pdf;', text)
            self.assertIn('StartupWMClass=org.enzim.Browser\n', text)
            wrapper = root / 'bin/enzim-browser-launch'
            self.assertTrue(wrapper.stat().st_mode & 0o111)
            self.assertIn('--binary', wrapper.read_text())
            self.assertIn(str(ROOT / 'tools/browser'), wrapper.read_text())
            self.assertFalse((root / 'data/applications/mimeapps.list').exists())


if __name__ == '__main__':
    unittest.main()
