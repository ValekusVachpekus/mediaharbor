# MediaHarbor

Локальная программа для скачивания видео и музыки с YouTube, поиска музыки по
названию или ссылке Spotify и сжатия медиафайлов. Приложение работает вместе с
расширением Chromium / Firefox. Сервер проекта и учётная запись MediaHarbor не нужны.

## Как запустить и получить установщики

Подробная инструкция: [INSTALL.md](INSTALL.md). На компьютере с доступом к сети
установщики для текущей ОС собираются одной командой:

```bash
npm run installers
```

Результаты появятся в `artifacts/installers/linux-x64/` (`.deb`, `.AppImage`)
или `artifacts/installers/windows-x64/` (`.exe`). Для обеих ОС одновременно
предусмотрен workflow **Actions → Installers → Run workflow**; он также запускается
при push в `main`/`master` и при отправке тегов `v*`.
На Arch/EndeavourOS добавьте собранный AppImage в меню командой
`python3 scripts/install-appimage.py` из корня проекта. Установка выполняется
для текущего пользователя; sudo не требуется.

## Возможности

- Видео MP4/MKV, аудио MP3/M4A/Opus или исходный аудиопоток.
- YouTube-плейлисты с выбором записей; подбор музыки и подтверждение совпадений.
- Spotify: метаданные одиночного трека, сбор плейлистов со страницы с прокруткой,
  CSV/TXT, необязательный собственный Spotify API Client ID и OAuth PKCE.
- Сжатие локальных файлов и после скачивания: пресеты, разрешение, битрейт, CRF,
  целевой размер и двухпроходное кодирование видео.
- Общая очередь: две загрузки и одно кодирование; отмена, повтор, история,
  сохранение исходников и восстановление прерванных задач.
- Кнопки на страницах, панель расширения и контекстное меню ссылок.
- Windows 10/11 x64 и Linux x64 (основная цель — Ubuntu 24.04+).

Spotify используется для определения записи. Пользователь выбирает соответствие
на YouTube; Spotify-аудио программа не извлекает. Доступность роликов зависит от
источника. Прямые эфиры и автоматический вход в YouTube в эту версию не входят.

## Запуск для разработки

Нужны Node.js 22.12+, npm, Rust 1.89+, Python 3.11+ для сборочных скриптов,
а также системные зависимости Tauri.

Ubuntu:

```bash
sudo apt-get install build-essential libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev
npm ci
```

Windows: установите Visual Studio C++ Build Tools и WebView2, затем выполните
`npm ci` из корня репозитория.

Инструменты можно установить в PATH или указать их каталог в настройках.
Для полного комплекта используйте проверку SHA-256 по файлу `tools.lock.json`:

```bash
# Только при первоначальном выборе / обновлении зависимостей:
python scripts/prepare-tools.py --resolve --platform linux-x64
# Проверьте версии, URL и хеши в tools.lock.json. Затем:
python scripts/prepare-tools.py --platform linux-x64

python scripts/prepare-sidecar.py --debug
npm run desktop
```

Для Windows используйте `--platform windows-x64`. В комплект входят yt-dlp,
FFmpeg, ffprobe и Deno. Современный standalone yt-dlp включает yt-dlp-ejs.
Обычная установка использует только зафиксированные URL и хеши; обновление
`latest` разрешается отдельной явной командой `--resolve`.

`npm run dev` открывает только интерфейс Vite на `127.0.0.1:1420`; доступ к
локальным файлам и загрузкам работает через Tauri (`npm run desktop`).

Зависимости JS и Rust зафиксированы в `package-lock.json` и `Cargo.lock`.
`tools.lock.json` первоначально пуст: `npm run installers` выберет версии бинарных
инструментов и сохранит URL и SHA-256 для текущей платформы. GitHub Actions явно
обновляет этот набор при каждой сборке; полученный lock входит в артефакты.
FFmpeg выбирается из датированного релиза, без изменяемого URL `latest`.
Для повторения сборки используйте полученный lock без `--refresh-tools`.

## Расширение

```bash
npm run extension:chrome
npm run extension:firefox
python scripts/extension-id.py
```

Если WXT ещё не установлен, те же entrypoints можно собрать небольшим адаптером
через esbuild: `npm run extension:portable`. Он использует тот же `wxt.config.ts`
и создаёт оба каталога `.output/*-mv3`; эта сборка проверена локально.

Chromium: откройте `chrome://extensions` или `edge://extensions`, включите режим
разработчика и загрузите каталог `apps/extension/.output/chrome-mv3`.
Включённый публичный ключ закрепляет ID `bkikhcebeennfdababfkhnmmojmafefp`.
В браузерах Chromium кроме Chrome/Edge может понадобиться регистрация host в
соответствующем профиле; автоматическая регистрация покрывает Chrome, Chromium и Edge.
Проверяемая матрица рассчитана на обычные установки браузеров; варианты Snap и
Flatpak в неё пока не включены.

Firefox: для разработки временно загрузите manifest из
`apps/extension/.output/firefox-mv3` через `about:debugging`.
Для постоянной установки в обычный Firefox требуется подписанный XPI, в том
числе при распространении вне магазина. ID: `mediaharbor@mediaharbor.local`.

Первый запуск программы регистрирует Native Messaging для текущего пользователя.
При необходимости повторите регистрацию в настройках или выполните:

```bash
target/debug/media-core --register bkikhcebeennfdababfkhnmmojmafefp
```

При регистрации ядро и встроенные инструменты копируются в постоянный каталог
пользователя. Это позволяет host работать независимо от временного монтирования
AppImage. Права администратора для регистрации не нужны.

Для Spotify-плейлиста откройте вкладку «Импорт Spotify» в расширении, запустите
сбор и оставьте страницу открытой. При неизвестном количестве или незагруженных
строках полнота списка не подтверждается. Можно импортировать найденные записи
с этим предупреждением либо использовать CSV / API. Виртуализированная разметка
сайта может меняться, поэтому импорт со страницы не считается гарантированным.

## Формат импорта и Spotify API

CSV UTF-8 с заголовком: `title,artist,url,duration`. Достаточно `title` или `url`;
`duration` задаётся в секундах. Поддерживаются кавычки, запятые и переносы внутри
полей. TXT: название, `исполнитель — название` или ссылка, по одной записи на строку.

```csv
title,artist,url,duration
Example song,Example artist,,222
```

API необязателен. В Spotify Developer Dashboard создайте собственное приложение,
добавьте точный redirect URI `http://127.0.0.1:43827/callback` и внесите Client ID
в настройки MediaHarbor. Client Secret не требуется и не сохраняется.
Refresh token хранится в Windows Credential Manager / Linux Secret Service.
Для Linux нужен работающий Secret Service, например GNOME Keyring или совместимый KWallet.

Development Mode зависит от Premium владельца API-приложения и разрешённых
пользователей. Чтение состава плейлистов ограничено правами Spotify, в частности
владением или соавторством. При HTTP 403/429 программа предлагает другой импорт
или ожидание, а не выдаёт пустой список за успешный результат.

## Сборки и проверки

```bash
npm test
npm run check
npm run build
npm run test:dom
npm run extension:portable
npm run test:extension
npm run check -w @mediaharbor/extension
cargo fmt --all -- --check
cargo test -p mediaharbor-core -- --include-ignored

# UI smoke с тестовым транспортом, требует установленного Chromium:
npx playwright install chromium
npm run test:ui
```

Изолированный набор использует **те же исходники** обработки, SQLite, очереди и
IPC без Tauri, HTTP и системного хранилища секретов:

```bash
cargo test --manifest-path tools/processing-tests/Cargo.toml --offline
```

Spotify и проверка обновлений в этом наборе заменены явными тестовыми реализациями. Проверка
жизненного цикла worker с настоящими сокетами помечена `ignored`; запустите её с
`--include-ignored` вне ограниченного sandbox. CI запускает и эту проверку.

Проверено здесь: TypeScript интерфейса, production-сборка Vite, парсер импорта,
сбор/объединение строк Spotify, Rust-модули обработки и очереди, сохранение SQLite,
отмена процессов, аутентифицированный IPC на потоке в памяти, реальные FFmpeg/ffprobe.
Полная сборка установщиков, OAuth, живые запросы YouTube/Spotify, запуск Chromium,
системные сокеты и установщики здесь не проверены: отсутствуют сеть либо необходимые
разрешения sandbox. Полная проверка Windows предусмотрена в матрице CI.
Дополнительно проверены настоящий React-интерфейс в DOM с тестовым транспортом,
обе portable-сборки расширения, его панель, маршрутизация сообщений и изоляция
content scripts от прямого доступа к Native Messaging.

Для установщика сначала подготовьте release-ядро и инструментальный lock:

```bash
python scripts/prepare-sidecar.py
# Linux:
npm run tauri -w @mediaharbor/desktop -- build --bundles deb,appimage
# Windows:
npm run tauri -w @mediaharbor/desktop -- build --bundles nsis
```

Перед публикацией выполните требования из `THIRD_PARTY_NOTICES.md`, приложите
соответствующие исходники GPL-компонентов и полные тексты лицензий. Firefox XPI
нужно подписать через Mozilla. Сам проект и релизные файлы распространяются через
GitHub; публикация в магазинах браузеров не является условием работы приложения.

## Данные и протокол

Исходники загрузок: `<папка результатов>/sources/<jobId>/`.
Временная обработка: `<папка результатов>/.mediaharbor-work/<jobId>/`.
Готовые файлы публикуются атомарно, без перезаписи; требуется файловая система
с hard links (например NTFS/ext4). После успешной обработки временные файлы удаляются.
При ошибке или отмене промежуточные данные сохраняются для диагностики/повтора.

Состояние Linux: `$XDG_DATA_HOME/mediaharbor` или `~/.local/share/mediaharbor`.
Windows: `%LOCALAPPDATA%\mediaharbor`. Для изоляции тестов поддерживается
`MEDIAHARBOR_DATA_DIR`; каталог инструментов можно задать через `MEDIAHARBOR_TOOLS_DIR`.

Протокол v1: `{version, requestId, op, payload}` →
`{version, requestId, ok, data?, error?: {code, message}}`.
Native Messaging использует длину little-endian + JSON, максимум 512 КиБ.
Коллекции и очередь передаются страницами; пакет создания содержит до 50 задач.
Повтор изменяющей команды с тем же `requestId` возвращает сохранённый ответ;
использование этого ID с другим содержимым отклоняется.
Прогресс интерфейсы получают опросом очереди каждые 2 секунды.

Разрешённые команды расширения не включают изменение настроек, произвольные
команды ОС, чтение локальных файлов и создание задач локального сжатия.
Секрет IPC защищён правами пользователя; открытый HTTP-порт для управления не используется.

Для удаления регистрации выполните `media-core --unregister` или
`python scripts/unregister-host.py`. Пользовательские файлы и история сохраняются.

Лицензия исходников: GPL-3.0-or-later.
