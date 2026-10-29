#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--debug', action='store_true')
args = parser.parse_args()
host = next(line.split(': ', 1)[1] for line in subprocess.check_output(['rustc', '-vV'], text=True).splitlines() if line.startswith('host: '))
command = ['cargo', 'build', '--locked', '-p', 'mediaharbor-core']
if not args.debug:
    command.append('--release')
subprocess.run(command, cwd=ROOT, check=True)
suffix = '.exe' if 'windows' in host else ''
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'], cwd=ROOT, text=True))
source = Path(metadata['target_directory']) / ('debug' if args.debug else 'release') / ('media-core' + suffix)
destination = ROOT / 'artifacts' / f'media-core-{host}{suffix}'
destination.parent.mkdir(exist_ok=True)
shutil.copy2(source, destination)
# The resource directory must exist for Tauri dev builds; tools may come from PATH.
(ROOT / 'artifacts/tools').mkdir(exist_ok=True)
print(destination)
