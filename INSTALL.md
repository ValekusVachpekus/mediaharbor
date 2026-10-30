# Запуск и установщики MediaHarbor

Собранные файлы находятся в `artifacts/installers/`. ZIP расширения не является
установщиком приложения. Для повторной сборки нужны зависимости и доступ к сети.

## Arch / EndeavourOS: добавить AppImage в меню

Запуск AppImage открывает переносимую программу. Чтобы она появилась в меню
приложений, из корня проекта выполните без sudo:

```bash
python3 scripts/install-appimage.py
```

Либо из каталога собранных установщиков:

```bash
python3 install-appimage.py
```

Скрипт копирует AppImage в `$XDG_DATA_HOME/mediaharbor/app/` (по умолчанию
`~/.local/share/mediaharbor/app/`), добавляет значок и ярлык в пользовательское
меню. Затем найдите **MediaHarbor** в меню приложений. Если оно уже открыто,
закройте и откройте его заново. История и загрузки сохраняются.

Можно указать другой исходный файл: `python3 scripts/install-appimage.py /путь/MediaHarbor.AppImage`.
Пакет `.deb` предназначен для Debian/Ubuntu; на Arch используйте AppImage.

## Получить установщики через GitHub Actions

Для выпущенных версий откройте
[Releases](https://github.com/ValekusVachpekus/mediaharbor/releases), скачайте из
**Assets** `mediaharbor-linux-x64-unsigned.zip` или
`mediaharbor-windows-x64-unsigned.zip` и распакуйте его. Инструкция установки
ниже подходит и для этих архивов. Полная инструкция: [README.md](README.md#установка-готовой-программы).

Для сборок ветки или если релизов ещё нет:

1. Загрузите исходники в свой репозиторий GitHub, включая каталог `.github`.
   Можно использовать `artifacts/mediaharbor-source.zip`; внутри нет `.git`,
   `node_modules`, локальных настроек и кешей.
2. Push изменений кода, ресурсов, тестов, зависимостей или настроек сборки
   в `main`/`master` запустит сборку автоматически; изменения только документации
   её не запускают. Теги `v*` запускают сборку релиза независимо от изменённых файлов.
   Вручную: **Actions → Installers → Run workflow**.
3. После успешного выполнения скачайте **Artifacts**:
   `mediaharbor-linux-x64-unsigned` и `mediaharbor-windows-x64-unsigned`.
4. Распакуйте скачанный архив. Внутри будут установщики, ZIP расширений,
   исходники проекта, версии компонентов и `SHA256SUMS.txt`.

Workflow сохраняет артефакты сборки. При push тега `v*` после успешной сборки обеих
ОС он также публикует два ZIP и `SHA256SUMS.txt` в GitHub Release для этого тега.
Для публикации отправьте новый тег: `git tag v0.1.0`, затем `git push origin v0.1.0`.
Если сборка завершилась ошибкой, установщики не считаются готовыми — причину
нужно смотреть в журнале шага **Build and verify installers**.

## Собрать на своём компьютере

Нужны Node.js 22.12+, Python 3.11+, Rust 1.89+ и доступ к интернету.

Для Ubuntu 24.04:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev patchelf libfuse2t64
python3 scripts/build-installers.py
```

Из корня проекта можно использовать одинаковую команду на обеих ОС:
`npm run installers`. Установка JS-зависимостей включена в сценарий;
заранее выполнять `npm install` для этой команды не обязательно.

Для Windows x64 установите Visual Studio Build Tools с компонентами C++,
Rust с toolchain MSVC, Node.js, Python и WebView2. Из корня проекта выполните:

```powershell
py -3 scripts/build-installers.py
```

Скрипт устанавливает JS-зависимости, фиксирует версии инструментов при первой
сборке, проверяет SHA-256 скачанных бинарников, запускает тесты и собирает приложение.
Повторная сборка использует `tools.lock.json`; для намеренного обновления инструментов
есть `--refresh-tools`. Зависимости JS/Rust устанавливаются по сохранённым npm/Cargo lockfiles.
GitHub Actions использует `--refresh-tools`: версии и SHA-256 инструментов
для конкретной сборки входят в скачанный артефакт. Для повторения такой сборки
используйте его `tools.lock.json` без `--refresh-tools`.

Результат:

- Linux: `artifacts/installers/linux-x64/` — `.deb` и `.AppImage`.
- Windows: `artifacts/installers/windows-x64/` — установщик NSIS `.exe`.

Каждая ОС собирается на собственном runner/компьютере. Windows `.exe` из Linux
этой командой не собирается.

## Запустить после установки

- **Windows:** запустите установщик `.exe`, затем откройте MediaHarbor из меню «Пуск».
- **Ubuntu/Debian:** установите `.deb` двойным щелчком или командой
  `sudo apt install ./имя-файла.deb`, затем откройте MediaHarbor из меню приложений.
- **AppImage:** разрешите исполнение файла в его свойствах и запустите двойным щелчком.
  Из терминала: `chmod +x ./имя-файла.AppImage`, затем `./имя-файла.AppImage`.

Node.js, Rust и Python для запуска установленной программы не нужны.
В установщик включаются yt-dlp, FFmpeg, ffprobe и Deno. На Linux нужны системные
библиотеки WebKitGTK и Secret Service для необязательного входа Spotify.

Расширение Chromium: распакуйте его ZIP, откройте `chrome://extensions`
или `edge://extensions`, включите режим разработчика и нажмите «Загрузить
распакованное расширение». Выберите папку с `manifest.json`.

Расширение Firefox: обычный Firefox требует подпись Mozilla. До подписи для
разработки используйте `about:debugging` → «Этот Firefox» → «Загрузить временное
дополнение» и выберите распакованный `manifest.json`.

Откройте приложение хотя бы один раз: оно зарегистрирует связь с браузером.
После этого можно передавать ссылки через расширение. При закрытии окна программа
остаётся в трее; пункт «Выйти и остановить задачи» завершает работу.

## Запуск исходников без установщика

После установки системных зависимостей из предыдущего раздела:

```bash
npm install
python3 scripts/prepare-tools.py --resolve --platform linux-x64
python3 scripts/prepare-tools.py --platform linux-x64
python3 scripts/prepare-sidecar.py --debug
npm run desktop
```

На Windows используйте `py -3` вместо `python3` и `windows-x64` вместо `linux-x64`.
`npm run dev` запускает только интерфейс; для загрузок нужна команда `npm run desktop`.

Подпись Windows и постоянная установка Firefox требуют отдельных сертификатов/
подписи Mozilla. Перед публичным распространением также выполните требования
`THIRD_PARTY_NOTICES.md` к лицензиям и соответствующим исходникам зависимостей.
