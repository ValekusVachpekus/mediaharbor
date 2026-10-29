import { browser } from 'wxt/browser';
import { completion, mergeTracks, spotifyTrack } from '../src/spotify-collector.mjs';
import type { CollectionItem } from '@mediaharbor/shared';

export default defineContentScript({
  matches: ['https://www.youtube.com/*', 'https://music.youtube.com/*', 'https://open.spotify.com/*'],
  main(ctx) {
    let scanning = false; let cancelled = false;
    function installButton() {
      if (document.getElementById('mediaharbor-action')) return;
      const spotify = location.hostname === 'open.spotify.com';
      const anchor = spotify ? document.querySelector('main [data-testid="action-bar-row"], main header, main h1') : document.querySelector('#top-level-buttons-computed, ytmusic-player-bar .right-controls');
      if (!anchor) return;
      const host = document.createElement('span'); host.id = 'mediaharbor-action';
      const shadow = host.attachShadow({ mode: 'open' });
      const style = document.createElement('style'); style.textContent = 'button{font:600 12px system-ui;background:#9dd9ae;color:#163328;border:0;padding:10px 14px;border-radius:20px;cursor:pointer;margin:6px}button:hover{background:#b4edc3}button:focus-visible{outline:2px solid white;outline-offset:2px}';
      const button = document.createElement('button'); button.textContent = '↓ MediaHarbor'; button.title = 'Открыть параметры загрузки';
      button.addEventListener('click', () => void browser.runtime.sendMessage({ type: 'page.open', source: location.href }));
      shadow.append(style, button); anchor.append(host);
    }
    const observer = new MutationObserver(() => { if (!document.getElementById('mediaharbor-action')) installButton(); });
    observer.observe(document.documentElement, { subtree: true, childList: true }); installButton();
    ctx.onInvalidated(() => { cancelled = true; observer.disconnect(); document.getElementById('mediaharbor-action')?.remove(); });
    browser.runtime.onMessage.addListener((message, _sender, sendResponse) => {
      if (message.type === 'scan.stop') { cancelled = true; sendResponse({ ok: true }); return; }
      if (message.type === 'scan.start') {
        if (scanning) { sendResponse({ ok: false, error: 'Сбор уже выполняется' }); return; }
        if (!/\/(playlist|album)\//.test(location.pathname)) { sendResponse({ ok: false, error: 'Откройте страницу плейлиста или альбома Spotify' }); return; }
        void collect(); sendResponse({ ok: true }); return;
      }
      return undefined;
    });
    async function collect() {
      scanning = true; cancelled = false;
      const source = location.href; const map = new Map<string, CollectionItem>();
      let items: CollectionItem[] = []; let expected: number | undefined; let stable = 0; let reachedEnd = false; let error = '';
      const main = document.querySelector('main') || document.body;
      let scroller = document.scrollingElement as HTMLElement;
      const seed = main.querySelector<HTMLElement>('[data-testid="tracklist-row"], [role="row"]') || main;
      for (let element: HTMLElement | null = seed as HTMLElement; element; element = element.parentElement) {
        if (element.scrollHeight > element.clientHeight + 30 && /auto|scroll/.test(getComputedStyle(element).overflowY)) { scroller = element; break; }
      }
      const original = scroller.scrollTop; scroller.scrollTop = 0;
      try {
        for (let round = 0; round < 1800 && !cancelled; round++) {
          if (location.href !== source) throw new Error('Страница изменилась. Импорт остановлен.');
          const rows = main.querySelectorAll<HTMLElement>('[data-testid="tracklist-row"], [role="row"]');
          const before = map.size;
          items = mergeTracks(map, [...rows].map(row => spotifyTrack(row)).filter(Boolean)) as CollectionItem[];
          const grid = main.querySelector('[aria-rowcount]');
          const rowCount = Number(grid?.getAttribute('aria-rowcount'));
          if (rowCount > 1) expected = rowCount - (grid?.querySelector('[role="columnheader"]') ? 1 : 0);
          if (!expected) {
            const header = main.querySelector('header')?.textContent || '';
            const count = header.match(/(\d[\d\s,]*)\s*(?:songs|tracks|трек[а-я]*|пес[а-я]*)/i);
            if (count?.[1]) expected = Number(count[1].replace(/[\s,]/g, ''));
          }
          stable = map.size === before ? stable + 1 : 0;
          reachedEnd = scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 8;
          await browser.storage.local.set({ spotifyImport: { source, items, expected, running: true, complete: false, message: `Найдено ${items.length}${expected ? ` из ${expected}` : ''}. Не закрывайте вкладку.` } });
          if ((reachedEnd && stable >= 8) || items.length >= 10000) break;
          scroller.scrollTop += Math.max(180, scroller.clientHeight * 0.7);
          await new Promise(resolve => setTimeout(resolve, 550));
        }
      } catch (e) { error = e instanceof Error ? e.message : String(e); }
      finally {
        const complete = !cancelled && !error && completion(items.length, expected, reachedEnd, stable);
        await browser.storage.local.set({ spotifyImport: { source, items, expected, running: false, complete, message: error || (cancelled ? 'Остановлено пользователем. Список может быть неполным.' : complete ? `Собрано треков: ${items.length}. Проверьте список перед загрузкой.` : `Найдено ${items.length}${expected ? ` из ${expected}` : ''}. Полнота списка не подтверждена. Можно импортировать найденное или использовать CSV / API.`) } });
        scroller.scrollTop = original; scanning = false;
      }
    }
  }
});
