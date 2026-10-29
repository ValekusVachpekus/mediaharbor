#!/usr/bin/env python3
"""Resolve upstream releases explicitly, then install only hash-locked assets.

python scripts/prepare-tools.py --resolve --platform linux-x64
python scripts/prepare-tools.py --platform linux-x64
Review and commit tools.lock.json after --resolve. Ordinary installs never resolve
"latest". Asset digests come from authenticated GitHub release metadata over TLS.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import tarfile
import tempfile
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / 'tools.lock.json'

def download(url):
    if not url.startswith(('https://api.github.com/', 'https://github.com/', 'https://raw.githubusercontent.com/')):
        raise ValueError('Unexpected download origin')
    headers = {'User-Agent': 'MediaHarbor-build/0.1', 'Accept': 'application/vnd.github+json'}
    token = os.environ.get('GITHUB_TOKEN')
    if token and url.startswith('https://api.github.com/'):
        headers['Authorization'] = f'Bearer {token}'
    with urllib.request.urlopen(urllib.request.Request(url, headers=headers), timeout=120) as response:
        return response.read()

def resolve(target):
    windows = target == 'windows-x64'
    definitions = [
        ('yt-dlp', 'yt-dlp/yt-dlp', 'yt-dlp.exe' if windows else 'yt-dlp_linux'),
        ('ffmpeg', 'BtbN/FFmpeg-Builds', f'ffmpeg-master-latest-{"win64" if windows else "linux64"}-gpl.' + ('zip' if windows else 'tar.xz')),
        ('deno', 'denoland/deno', f'deno-x86_64-{"pc-windows-msvc" if windows else "unknown-linux-gnu"}.zip'),
    ]
    result = []
    for tool, repository, asset_name in definitions:
        if tool == 'ffmpeg':
            # The "latest" tag is overwritten daily. Use a dated release and
            # its versioned asset so the URL and digest refer to the same build.
            releases = json.loads(download(f'https://api.github.com/repos/{repository}/releases?per_page=30'))
            release = next((r for r in releases if r['tag_name'].startswith('autobuild-')
                            and not r.get('draft') and not r.get('prerelease')), None)
            if release is None:
                raise RuntimeError(f'{repository}: no dated release found')
            ending = r'win64-gpl\.zip' if windows else r'linux64-gpl\.tar\.xz'
            asset = next((a for a in release['assets'] if re.fullmatch(r'ffmpeg-N-.+-' + ending, a['name'])), None)
        else:
            release = json.loads(download(f'https://api.github.com/repos/{repository}/releases/latest'))
            asset = next((a for a in release['assets'] if a['name'] == asset_name), None)
        if not asset:
            raise RuntimeError(f'{repository}: asset {asset_name} not found; review upstream packaging')
        asset_name = asset['name']
        digest = asset.get('digest', '')
        if not digest.startswith('sha256:'):
            raise RuntimeError(f'{repository}: no SHA-256 digest; supply a reviewed lock entry instead')
        result.append({'tool': tool, 'repository': repository, 'version': release['tag_name'], 'asset': asset_name, 'url': asset['browser_download_url'], 'sha256': digest[7:], 'source': f'https://github.com/{repository}/tree/{release["tag_name"]}'})
    lock = json.loads(LOCK.read_text())
    lock['platforms'][target] = result
    LOCK.write_text(json.dumps(lock, indent=2) + '\n')
    print(f'Resolved {target}. Review {LOCK.name} before distributing binaries.')

def install(target):
    entries = json.loads(LOCK.read_text())['platforms'].get(target)
    if not entries:
        raise RuntimeError(f'No lock for {target}. Run --resolve explicitly and review the resulting versions.')
    output = ROOT / 'artifacts/tools'
    output.mkdir(parents=True, exist_ok=True)
    suffix = '.exe' if target == 'windows-x64' else ''
    notices = []
    for entry in entries:
        raw = download(entry['url'])
        if hashlib.sha256(raw).hexdigest() != entry['sha256']:
            raise RuntimeError(f'SHA-256 mismatch: {entry["asset"]}')
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            archive = temp / entry['asset']
            archive.write_bytes(raw)
            required = ['ffmpeg', 'ffprobe'] if entry['tool'] == 'ffmpeg' else [entry['tool']]
            if entry['asset'].endswith('.zip'):
                with zipfile.ZipFile(archive) as z:
                    # Never extract arbitrary paths from an upstream archive.
                    for info in z.infolist():
                        name = Path(info.filename).name
                        if name in [n + suffix for n in required]:
                            (output / name).write_bytes(z.read(info))
                        elif 'license' in name.lower() or 'copying' in name.lower():
                            (output / f'{entry["tool"]}-{name}').write_bytes(z.read(info))
            elif entry['asset'].endswith('.tar.xz'):
                with tarfile.open(archive) as tar:
                    for info in tar.getmembers():
                        name = Path(info.name).name
                        if info.isfile() and (name in required or 'license' in name.lower() or 'copying' in name.lower()):
                            data = tar.extractfile(info).read()
                            (output / (name if name in required else f'{entry["tool"]}-{name}')).write_bytes(data)
            else:
                (output / (entry['tool'] + suffix)).write_bytes(raw)
            for name in required:
                path = output / (name + suffix)
                if not path.is_file():
                    raise RuntimeError(f'Archive did not contain {name}')
                path.chmod(0o755)
        notices.append(entry)
        print(f'Installed {entry["tool"]} {entry["version"]}')
    (output / 'DEPENDENCIES.json').write_text(json.dumps(notices, indent=2) + '\n')
    shutil.copyfile(ROOT / 'THIRD_PARTY_NOTICES.md', output / 'THIRD_PARTY_NOTICES.md')

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', choices=['linux-x64', 'windows-x64'], default='windows-x64' if platform.system() == 'Windows' else 'linux-x64')
    parser.add_argument('--resolve', action='store_true')
    parser.add_argument('--check', action='store_true', help='Print locked versions without downloading')
    args = parser.parse_args()
    if args.resolve:
        resolve(args.platform)
    elif args.check:
        print(json.dumps(json.loads(LOCK.read_text())['platforms'].get(args.platform, []), indent=2))
    else:
        install(args.platform)
