import { browser } from 'wxt/browser';
import type { Request, Response } from '@mediaharbor/shared';

const HOST = 'io.mediaharbor.host';
const ids = new Set(['mediaharbor-link', 'mediaharbor-page']);
function nativeRequest(req: Request): Promise<Response> {
  return new Promise((resolve, reject) => {
    const port = browser.runtime.connectNative(HOST);
    let finished = false;
    const timer = setTimeout(() => finish(undefined, new Error('Истекло время ожидания приложения')), 180000);
    function finish(response?: Response, error?: Error) {
      if (finished) return;
      finished = true; clearTimeout(timer); port.disconnect();
      if (error) reject(error); else resolve(response!);
    }
    port.onMessage.addListener(response => {
      if (response?.requestId !== req.requestId || response?.version !== 1) return finish(undefined, new Error('Несовместимый ответ приложения'));
      finish(response);
    });
    port.onDisconnect.addListener(() => { const message = browser.runtime.lastError?.message; finish(undefined, new Error(message || 'Соединение с приложением закрыто')); });
    port.postMessage(req);
  });
}
export default defineBackground(() => {
  browser.runtime.onInstalled.addListener(() => {
    browser.contextMenus.removeAll().then(async () => {
      await browser.contextMenus.create({ id: 'mediaharbor-link', title: 'Открыть ссылку в MediaHarbor', contexts: ['link'], targetUrlPatterns: ['https://*.youtube.com/*', 'https://youtu.be/*', 'https://open.spotify.com/*', 'https://spotify.link/*'] });
      await browser.contextMenus.create({ id: 'mediaharbor-page', title: 'Сохранить через MediaHarbor', contexts: ['page'], documentUrlPatterns: ['https://www.youtube.com/*', 'https://music.youtube.com/*', 'https://open.spotify.com/*'] });
    }).catch(console.error);
  });
  async function prepare(source: string) {
    await browser.storage.local.set({ pendingSource: source });
    try { await browser.action.openPopup(); } catch { await browser.action.setBadgeText({ text: '↓' }); await browser.action.setBadgeBackgroundColor({ color: '#37634a' }); }
  }
  browser.contextMenus.onClicked.addListener(info => { if (ids.has(String(info.menuItemId))) void prepare(info.linkUrl || info.pageUrl || ''); });
  browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
    let work: Promise<unknown> | undefined;
    if (message?.type === 'page.open' && sender.tab) { work = prepare(String(message.source || sender.tab.url || '')); }
    // Only extension pages can reach the native host or start a scan.
    else if (sender.tab || !sender.url?.startsWith(browser.runtime.getURL(''))) return undefined;
    else if (message?.type === 'rpc') {
      const req = message.request as Request;
      work = nativeRequest(req).catch((): Response => ({ version: 1, requestId: req.requestId, ok: false, error: { code: 'HOST_UNAVAILABLE', message: 'Приложение не подключено. Установите MediaHarbor и зарегистрируйте Native Messaging в настройках программы.' } }));
    }
    else if (message?.type === 'scan.start' || message?.type === 'scan.stop') {
      work = browser.tabs.query({ active: true, currentWindow: true }).then(async ([tab]) => {
        if (!tab?.id || !tab.url?.startsWith('https://open.spotify.com/')) throw new Error('Откройте плейлист на open.spotify.com');
        return browser.tabs.sendMessage(tab.id, { type: message.type });
      });
    }
    if (!work) return undefined;
    work.then(value => sendResponse(value ?? { ok: true })).catch(error => sendResponse({ ok: false, error: error instanceof Error ? error.message : String(error) }));
    return true;
  });
});
