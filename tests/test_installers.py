import hashlib
import importlib.util
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SCRIPT = Path(__file__).resolve().parent.parent / 'scripts/build-installers.py'
SPEC = importlib.util.spec_from_file_location('installer_build', SCRIPT)
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class InstallerTests(unittest.TestCase):
    def test_cyrillic_output_with_redirected_windows_encoding(self):
        # Exercise the entrypoint in a fresh interpreter whose redirected
        # streams initially use cp1252, as on the failing Windows runner.
        code = '''
import importlib.util
import os
import sys
from unittest.mock import patch
spec = importlib.util.spec_from_file_location('installer_build', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
fail = sys.argv[2] == 'error'
def check():
    if fail:
        raise RuntimeError('Проверка ошибки')
    print('Платформа: windows-x64')
    assert os.environ['PYTHONUTF8'] == '1'
    assert os.environ['PYTHONIOENCODING'] == 'utf-8'
    return 'windows-x64', ('nsis',)
sys.argv = [sys.argv[1], '--check']
with patch.object(module, 'check_environment', side_effect=check):
    sys.exit(module.main())
'''
        env = {**os.environ, 'PYTHONUTF8': '0', 'PYTHONIOENCODING': 'cp1252'}
        for mode in ('success', 'error'):
            with self.subTest(mode=mode):
                result = subprocess.run([sys.executable, '-c', code, str(SCRIPT), mode],
                                        capture_output=True, env=env)
                self.assertEqual(result.returncode, 1 if mode == 'error' else 0)
                if mode == 'error':
                    self.assertIn('Сборка не завершена: Проверка ошибки', result.stderr.decode('utf-8'))
                else:
                    self.assertIn('Платформа: windows-x64', result.stdout.decode('utf-8'))

    def test_selects_native_bundles_and_rejects_unsupported_hosts(self):
        self.assertEqual(BUILD.host_platform('Linux', 'x86_64'), ('linux-x64', ('deb', 'appimage')))
        self.assertEqual(BUILD.host_platform('Windows', 'AMD64'), ('windows-x64', ('nsis',)))
        for system, machine in [('Darwin', 'x86_64'), ('Linux', 'aarch64')]:
            with self.assertRaises(RuntimeError):
                BUILD.host_platform(system, machine)

    def test_source_archive_excludes_secrets_caches_and_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            names = [
                'README.md', 'apps/desktop/src/App.tsx', '.github/workflows/installers.yml',
                '.aws/credentials', '.env', 'apps/desktop/.env.local',
                'apps/desktop/.npmrc', 'apps/desktop/node_modules/pkg/index.js',
                'tools/processing-tests/target/binary', 'scripts/__pycache__/file.pyc',
                'apps/desktop/.git/config',
            ]
            for name in names:
                file = root / name
                file.parent.mkdir(parents=True, exist_ok=True)
                file.write_text('test', encoding='utf-8')
            try:
                (root / 'scripts/secret-link').symlink_to(root / '.aws/credentials')
            except OSError:
                pass
            archive = BUILD.source_archive(root / 'source.zip', root)
            with zipfile.ZipFile(archive) as zipped:
                self.assertEqual(set(zipped.namelist()), {
                    'README.md', 'apps/desktop/src/App.tsx', '.github/workflows/installers.yml',
                })

    def test_collect_requires_every_bundle_and_rejects_empty_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / 'output'
            output.mkdir()
            deb = root / 'release/bundle/deb/test.deb'
            deb.parent.mkdir(parents=True)
            deb.write_bytes(b'test artifact')
            with self.assertRaises(RuntimeError):
                BUILD.collect_bundles(root, output, ('deb', 'appimage'))
            image = root / 'release/bundle/appimage/test.AppImage'
            image.parent.mkdir(parents=True)
            image.touch()
            with self.assertRaisesRegex(RuntimeError, 'Пустой'):
                BUILD.collect_bundles(root, output, ('deb', 'appimage'))
            image.write_bytes(b'test image')
            files = BUILD.collect_bundles(root, output, ('deb', 'appimage'))
            self.assertEqual(len(files), 2)
            self.assertEqual(BUILD.digest(output / 'test.deb'), hashlib.sha256(deb.read_bytes()).hexdigest())

    def test_missing_dns_stops_before_building(self):
        with patch.object(BUILD, 'host_platform', return_value=('windows-x64', ('nsis',))), \
                patch.object(BUILD.shutil, 'which', return_value='/tool'), \
                patch.object(BUILD.subprocess, 'check_output', return_value='v22.12.0'), \
                patch.object(BUILD.socket, 'getaddrinfo', side_effect=socket.gaierror('DNS unavailable')):
            with self.assertRaisesRegex(RuntimeError, 'установщики не созданы'):
                BUILD.check_environment()


if __name__ == '__main__':
    unittest.main()
