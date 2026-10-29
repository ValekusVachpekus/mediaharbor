import { defineConfig } from 'wxt';
import { readFileSync } from 'node:fs';
export default defineConfig({
  modules: ['@wxt-dev/module-react'],
  manifest: ({ browser }) => ({
    name: 'MediaHarbor', description: 'Загрузка через локальное приложение и импорт музыкальных коллекций.', version: '0.1.0',
    permissions: ['nativeMessaging', 'storage', 'contextMenus', 'activeTab'],
    host_permissions: ['https://www.youtube.com/*', 'https://music.youtube.com/*', 'https://open.spotify.com/*'],
    key: browser === 'firefox' ? undefined : readFileSync(new URL('./public/chromium-key.txt', import.meta.url), 'utf8').trim(),
    browser_specific_settings: browser === 'firefox' ? { gecko: { id: 'mediaharbor@mediaharbor.local', strict_min_version: '140.0', data_collection_permissions: { required: ['none'] } } } : undefined,
    action: { default_title: 'MediaHarbor' },
    icons: { 48: '/icon/48.png', 128: '/icon/128.png' }
  })
});
