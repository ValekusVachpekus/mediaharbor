#!/usr/bin/env python3
"""Install MediaHarbor AppImage into the current user's application menu."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def desktop_argument(value):
    if any(ord(char) < 32 for char in value):
        raise ValueError('Путь содержит управляющие символы.')
    # Exec quoting is parsed after Desktop Entry string escaping.
    quoted = ''.join('\\' + char if char in '\\"`$' else '%%' if char == '%' else char for char in value)
    return '"' + quoted.replace('\\', '\\\\') + '"'


def default_image():
    script = Path(__file__).resolve()
    candidates = list(script.parent.glob('MediaHarbor*.AppImage'))
    candidates += list((script.parent.parent / 'artifacts/installers/linux-x64').glob('MediaHarbor*.AppImage'))
    candidates = sorted(set(path.resolve() for path in candidates if path.is_file()))
    if len(candidates) != 1:
        raise ValueError('Укажите путь к AppImage: python3 scripts/install-appimage.py /путь/MediaHarbor.AppImage')
    return candidates[0]


def default_icon():
    script = Path(__file__).resolve()
    for icon in (script.with_name('mediaharbor.png'), script.parent.parent / 'apps/desktop/src-tauri/icons/256x256.png'):
        if icon.is_file():
            return icon
    return None


def write_atomic(path, data, mode):
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        try:
            output.write(data)
            output.flush()
            os.fchmod(output.fileno(), mode)
        except BaseException:
            temporary.unlink(missing_ok=True)
            raise
    try:
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def install(image, data_home, icon=None):
    image = Path(image).resolve(strict=True)
    if not image.is_file() or image.suffix != '.AppImage':
        raise ValueError('Нужен существующий файл .AppImage.')
    with image.open('rb') as source:
        if source.read(4) != b'\x7fELF':
            raise ValueError('Выбранный файл не является исполняемым AppImage.')
    data_home = Path(data_home).expanduser().resolve()
    destination = data_home / 'mediaharbor/app/MediaHarbor.AppImage'
    destination.parent.mkdir(parents=True, exist_ok=True)
    if image != destination:
        with tempfile.NamedTemporaryFile(dir=destination.parent, delete=False) as output:
            temporary = Path(output.name)
            try:
                with image.open('rb') as source:
                    shutil.copyfileobj(source, output)
                output.flush()
                os.fchmod(output.fileno(), 0o755)
            except BaseException:
                temporary.unlink(missing_ok=True)
                raise
        try:
            os.replace(temporary, destination)
        finally:
            temporary.unlink(missing_ok=True)
    else:
        destination.chmod(0o755)
    icon_name = 'application-x-executable'
    if icon:
        write_atomic(data_home / 'icons/hicolor/256x256/apps/io.mediaharbor.app.png', Path(icon).read_bytes(), 0o644)
        icon_name = 'io.mediaharbor.app'
    launcher = data_home / 'applications/io.mediaharbor.app.desktop'
    entry = (
        '[Desktop Entry]\nType=Application\nVersion=1.0\nName=MediaHarbor\n'
        'Comment=Загрузка и сжатие видео и музыки\n'
        f'Exec=/usr/bin/env {desktop_argument(str(destination))}\n'
        f'Icon={icon_name}\nTerminal=false\nCategories=AudioVideo;Video;\n'
        'Keywords=YouTube;Spotify;Download;Видео;Музыка;\n'
        'StartupWMClass=mediaharbor-desktop\n'
    )
    write_atomic(launcher, entry.encode('utf-8'), 0o644)
    return destination, launcher


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image', nargs='?', type=Path)
    parser.add_argument('--data-dir', type=Path, help='Каталог XDG data; по умолчанию ~/.local/share')
    args = parser.parse_args()
    try:
        if sys.platform != 'linux':
            raise ValueError('Этот способ установки предназначен для Linux.')
        image = args.image or default_image()
        data_home = args.data_dir or Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share')
        executable, launcher = install(image, data_home, default_icon())
        updater = shutil.which('update-desktop-database')
        if updater:
            subprocess.run([updater, str(launcher.parent)], check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        print(f'MediaHarbor добавлен в меню приложений.\nПрограмма: {executable}\nЯрлык: {launcher}')
        print('Найдите MediaHarbor в меню. Если меню уже открыто, закройте и откройте его заново.')
    except (OSError, ValueError) as error:
        print(f'Не удалось добавить приложение: {error}', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
