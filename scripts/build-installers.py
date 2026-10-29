#!/usr/bin/env python3
"""Build native installers with their tools and browser extensions.

Run on Linux for DEB/AppImage or on Windows for NSIS. Nothing is published.
The first build resolves tools.lock.json; later builds reuse the locked digests.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import socket
import subprocess
import sys
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE_ROOTS = ('apps', 'crates', 'packages', 'scripts', 'tests', 'tools', '.github')
SOURCE_FILES = (
    'Cargo.toml', 'Cargo.lock', 'package.json', 'package-lock.json',
    'tsconfig.json', 'tools.lock.json', 'README.md', 'INSTALL.md',
    'LICENSE', 'COPYING', 'THIRD_PARTY_NOTICES.md', '.gitignore', '.gitattributes',
)
EXCLUDED = {
    'node_modules', 'target', 'dist', '.output', '.wxt', '__pycache__',
    '.git', '.aws', '.codex', '.agents', '.venv', '.cache',
}


def host_platform(system=None, machine=None):
    system = system or platform.system()
    machine = (machine or platform.machine()).lower()
    if machine not in ('x86_64', 'amd64'):
        raise RuntimeError('Поддерживаются только сборки x64.')
    if system == 'Linux':
        return 'linux-x64', ('deb', 'appimage')
    if system == 'Windows':
        return 'windows-x64', ('nsis',)
    raise RuntimeError('Собирайте на Linux или Windows. Кросс-сборка установщиков не включена.')


def run(args):
    print('+ ' + ' '.join(map(str, args)), flush=True)
    command = list(map(str, args))
    # Invoke npm's JS entrypoint on Windows: CreateProcess cannot directly run
    # a .cmd launcher. This also avoids an intermediate command shell.
    if os.name == 'nt' and command[0] == 'npm':
        npm = shutil.which('npm.cmd') or shutil.which('npm')
        node = shutil.which('node')
        if not npm or not node:
            raise RuntimeError('Не найдены Node.js/npm.')
        candidates = [
            Path(npm).parent / 'node_modules/npm/bin/npm-cli.js',
            Path(node).parent / 'node_modules/npm/bin/npm-cli.js',
        ]
        entrypoint = next((path for path in candidates if path.is_file()), None)
        if entrypoint is None:
            raise RuntimeError('Не найден npm-cli.js. Установите стандартную сборку Node.js с npm.')
        command = [node, str(entrypoint), *command[1:]]
    build_env = os.environ.copy()
    tools = ROOT / 'artifacts/tools'
    if tools.is_dir():
        build_env['MEDIAHARBOR_TOOLS_DIR'] = str(tools)
        build_env['PATH'] = str(tools) + os.pathsep + build_env.get('PATH', '')
    subprocess.run(command, cwd=ROOT, check=True, env=build_env)


def check_environment(require_network=True):
    target, bundles = host_platform()
    missing = [name for name in ('node', 'npm', 'cargo', 'rustc') if not shutil.which(name)]
    if missing:
        raise RuntimeError('Не найдены: ' + ', '.join(missing) + '. См. INSTALL.md.')
    if sys.version_info < (3, 11):
        raise RuntimeError('Требуется Python 3.11 или новее.')
    node_version = subprocess.check_output(['node', '--version'], text=True).strip()
    node_parts = tuple(map(int, node_version.lstrip('v').split('.')[:2]))
    if node_parts < (22, 12):
        raise RuntimeError('Требуется Node.js 22.12 или новее.')
    if target == 'linux-x64':
        if not shutil.which('pkg-config'):
            raise RuntimeError('Не найден pkg-config. Системные зависимости перечислены в INSTALL.md.')
        result = subprocess.run(
            ['pkg-config', '--exists', 'gtk+-3.0', 'webkit2gtk-4.1', 'dbus-1', 'ayatana-appindicator3-0.1'],
            capture_output=True,
        )
        if result.returncode:
            raise RuntimeError('Отсутствуют системные библиотеки Tauri. См. команду apt в INSTALL.md.')
    if require_network:
        unreachable = []
        for name in ('registry.npmjs.org', 'index.crates.io', 'api.github.com', 'github.com'):
            try:
                socket.getaddrinfo(name, 443, type=socket.SOCK_STREAM)
            except OSError:
                unreachable.append(name)
        if unreachable:
            raise RuntimeError(
                'DNS недоступен для ' + ', '.join(unreachable)
                + '. Зависимости и инструменты не скачать; установщики не созданы.'
            )
    print(f'Платформа: {target}; установщики: {", ".join(bundles)}', flush=True)
    return target, bundles


def source_archive(destination, root=ROOT):
    """Allowlist sources; never include credentials, local data or build caches."""
    root = Path(root)
    files = [root / name for name in SOURCE_FILES if (root / name).is_file()]
    for name in SOURCE_ROOTS:
        directory = root / name
        if not directory.is_dir() or directory.is_symlink():
            continue
        for directory_path, subdirs, names in os.walk(directory, followlinks=False):
            base = Path(directory_path)
            subdirs[:] = sorted(d for d in subdirs if d not in EXCLUDED and not (base / d).is_symlink())
            for filename in sorted(names):
                path = base / filename
                if (path.is_symlink() or filename.startswith('.env') or filename == '.npmrc'
                        or path.suffix in ('.pyc', '.log') or not path.is_file()):
                    continue
                files.append(path)
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, 'w', zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(set(files)):
            if not path.is_symlink():
                archive.write(path, path.relative_to(root).as_posix())
    return destination


def digest(path):
    with Path(path).open('rb') as file:
        return hashlib.file_digest(file, 'sha256').hexdigest()


def collect_bundles(target_directory, destination, bundles):
    suffixes = {'deb': '.deb', 'appimage': '.AppImage', 'nsis': '.exe'}
    copied = []
    for bundle in bundles:
        directory = Path(target_directory) / 'release' / 'bundle' / bundle
        files = sorted(directory.glob('*' + suffixes[bundle]))
        if not files:
            raise RuntimeError(f'Tauri не создал {bundle}: {directory}. Успех сборки не подтверждён.')
        for file in files:
            if file.stat().st_size == 0:
                raise RuntimeError(f'Пустой установщик: {file}')
            destination_file = Path(destination) / file.name
            shutil.copy2(file, destination_file)
            copied.append(destination_file)
    return copied


def build(args):
    target, bundles = check_environment()
    if args.check:
        return
    if not args.skip_install:
        run(['npm', 'ci' if (ROOT / 'package-lock.json').exists() else 'install', '--no-audit', '--no-fund'])
    lock = json.loads((ROOT / 'tools.lock.json').read_text(encoding='utf-8'))
    if args.refresh_tools or not lock['platforms'].get(target):
        print('Фиксирую версии и SHA-256 инструментов в tools.lock.json.', flush=True)
        run([sys.executable, 'scripts/prepare-tools.py', '--resolve', '--platform', target])
    run([sys.executable, 'scripts/prepare-tools.py', '--platform', target])
    run(['npm', 'run', 'check'])
    run(['npm', 'test'])
    run(['npm', 'run', 'build'])
    run(['npm', 'run', 'test:dom'])
    run(['npm', 'run', 'extension:portable'])
    run(['npm', 'run', 'test:extension'])
    run(['cargo', 'test', '--locked', '-p', 'mediaharbor-core', '--', '--include-ignored'])
    run([sys.executable, 'scripts/prepare-sidecar.py'])
    # Remove stale installers so that a failed/empty build cannot reuse an old
    # artifact and be reported as a successful installer build.
    target_directory = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    if not target_directory.is_absolute():
        target_directory = ROOT / target_directory
    for bundle in bundles:
        directory = target_directory / 'release' / 'bundle' / bundle
        if directory.exists():
            shutil.rmtree(directory)
    run(['npm', 'run', 'tauri', '-w', '@mediaharbor/desktop', '--', 'build', '--bundles', ','.join(bundles)])
    destination = ROOT / 'artifacts' / 'installers' / target
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    files = collect_bundles(target_directory, destination, bundles)
    if target == 'linux-x64':
        for source, name in ((ROOT / 'scripts/install-appimage.py', 'install-appimage.py'),
                             (ROOT / 'apps/desktop/src-tauri/icons/256x256.png', 'mediaharbor.png')):
            shutil.copy2(source, destination / name)
            files.append(destination / name)
    for browser in ('chrome', 'firefox'):
        directory = ROOT / 'apps/extension/.output' / f'{browser}-mv3'
        filename = destination / f'mediaharbor-{browser}-unsigned.zip'
        shutil.make_archive(str(filename.with_suffix('')), 'zip', directory)
        files.append(filename)
    files.append(source_archive(destination / 'mediaharbor-source.zip'))
    for name in ('INSTALL.md', 'tools.lock.json', 'LICENSE', 'COPYING', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT / name, destination / name)
        files.append(destination / name)
    checksums = destination / 'SHA256SUMS.txt'
    checksums.write_text(''.join(f'{digest(file)}  {file.name}\n' for file in sorted(files)), encoding='utf-8')
    print(f'\nУстановщики созданы: {destination}', flush=True)
    print('Сборки не подписаны цифровым сертификатом; Firefox ZIP требует подписи Mozilla.', flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Проверить зависимости и DNS без запуска сборки')
    parser.add_argument('--skip-install', action='store_true', help='Не запускать npm install/ci')
    parser.add_argument('--refresh-tools', action='store_true', help='Обновить зафиксированные версии инструментов')
    parser.add_argument('--source-only', action='store_true', help='Упаковать исходники без сети и компиляции')
    args = parser.parse_args()
    try:
        if args.source_only:
            print(source_archive(ROOT / 'artifacts/mediaharbor-source.zip'))
        else:
            build(args)
    except (OSError, RuntimeError, subprocess.CalledProcessError, ValueError) as error:
        print(f'Сборка не завершена: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
