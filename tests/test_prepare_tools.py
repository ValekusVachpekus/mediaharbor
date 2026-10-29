import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    'prepare_tools', Path(__file__).resolve().parent.parent / 'scripts/prepare-tools.py')
TOOLS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOLS)


class PrepareToolsTests(unittest.TestCase):
    def test_resolve_uses_dated_ffmpeg_assets_on_both_platforms(self):
        for target, suffix in [('linux-x64', 'linux64-gpl.tar.xz'),
                               ('windows-x64', 'win64-gpl.zip')]:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                lock = Path(temporary) / 'tools.lock.json'
                lock.write_text('{"schema": 1, "platforms": {}}')

                def download(url):
                    if 'FFmpeg-Builds' in url:
                        return json.dumps([
                            {'tag_name': 'latest', 'assets': []},
                            {'tag_name': 'autobuild-2026-10-28-19-47', 'assets': [
                                {'name': f'ffmpeg-N-128142-g28fda48e0b-{suffix}',
                                 'digest': 'sha256:' + 'a' * 64,
                                 'browser_download_url': 'https://github.com/example/dated-ffmpeg'}]},
                        ]).encode()
                    windows = target == 'windows-x64'
                    name = ('yt-dlp.exe' if windows else 'yt-dlp_linux') if 'yt-dlp' in url else (
                        'deno-x86_64-pc-windows-msvc.zip' if windows else 'deno-x86_64-unknown-linux-gnu.zip')
                    return json.dumps({'tag_name': 'v1', 'assets': [
                        {'name': name, 'digest': 'sha256:' + 'b' * 64,
                         'browser_download_url': 'https://github.com/example/' + name}]}).encode()

                with patch.object(TOOLS, 'LOCK', lock), patch.object(TOOLS, 'download', side_effect=download):
                    TOOLS.resolve(target)
                entry = json.loads(lock.read_text())['platforms'][target][1]
                self.assertEqual(entry['version'], 'autobuild-2026-10-28-19-47')
                self.assertTrue(entry['asset'].endswith(suffix))
                self.assertEqual(entry['sha256'], 'a' * 64)


if __name__ == '__main__':
    unittest.main()
