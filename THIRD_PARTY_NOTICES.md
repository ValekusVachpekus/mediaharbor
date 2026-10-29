# Third-party components

MediaHarbor source is GPL-3.0-or-later. Each dependency retains its license.

| Component | License / source |
| --- | --- |
| Tauri | MIT OR Apache-2.0 — https://github.com/tauri-apps/tauri |
| React | MIT — https://github.com/facebook/react |
| WXT | MIT — https://github.com/wxt-dev/wxt |
| yt-dlp | Unlicense for source; bundled standalone distributions include other components — https://github.com/yt-dlp/yt-dlp/blob/master/LICENSE |
| yt-dlp-ejs | Unlicense with bundled MIT/ISC components — https://github.com/yt-dlp/ejs |
| FFmpeg GPL builds / libx264 | GPL; exact version/configuration and upstream source links must accompany the binary — https://github.com/BtbN/FFmpeg-Builds |
| Deno | MIT and third-party licenses — https://github.com/denoland/deno |
| SQLite | Public domain — https://sqlite.org/copyright.html |

The tool installer preserves license/copying files from upstream archives and
records release URLs and SHA-256 hashes in `artifacts/tools/DEPENDENCIES.json`.
Before publishing an installer, include the complete applicable license notices
for standalone yt-dlp and Deno, and provide corresponding source and build scripts
for the exact GPL FFmpeg build and its linked libraries. The GitHub workflow
produces development artifacts; it does not publish installers or imply that
these redistribution requirements have already been completed.

No vendor endorsement is implied. Spotify metadata retains its attribution and
original links; no Spotify audio is extracted.
