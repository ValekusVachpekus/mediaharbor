import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('appimage_install', Path(__file__).resolve().parent.parent / 'scripts/install-appimage.py')
INSTALL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INSTALL)


@unittest.skipUnless(sys.platform == 'linux', 'AppImage installation targets Linux')
class AppImageInstallTests(unittest.TestCase):
    def test_menu_entry_and_update_preserve_user_data(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / 'Source.AppImage'
            image.write_bytes(b'\x7fELF' + b'test fixture')
            data = root / 'data with spaces'
            history = data / 'mediaharbor/library.sqlite'
            history.parent.mkdir(parents=True)
            history.write_bytes(b'history')
            executable, desktop = INSTALL.install(image, data)
            self.assertEqual(executable.read_bytes(), image.read_bytes())
            self.assertTrue(executable.stat().st_mode & 0o111)
            self.assertIn(f'Exec=/usr/bin/env "{executable}"', desktop.read_text())
            self.assertEqual(history.read_bytes(), b'history')
            image.write_bytes(b'\x7fELFupdated fixture')
            INSTALL.install(image, data)
            self.assertEqual(executable.read_bytes(), image.read_bytes())
            self.assertEqual(history.read_bytes(), b'history')
            if shutil.which('desktop-file-validate'):
                subprocess.run(['desktop-file-validate', str(desktop)], check=True)

    def test_invalid_file_does_not_create_launcher(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            image = root / 'Invalid.AppImage'
            image.write_bytes(b'not an executable')
            with self.assertRaises(ValueError):
                INSTALL.install(image, root / 'data')
            self.assertFalse((root / 'data/applications').exists())

    def test_exec_escapes_special_characters_and_rejects_newlines(self):
        argument = INSTALL.desktop_argument('/tmp/a$b`c%file.AppImage')
        self.assertIn('\\\\$', argument)
        self.assertIn('%%', argument)
        with self.assertRaises(ValueError):
            INSTALL.desktop_argument('/tmp/file\nExec=bad')


if __name__ == '__main__':
    unittest.main()
