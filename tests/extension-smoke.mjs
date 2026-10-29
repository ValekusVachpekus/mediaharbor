import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import vm from 'node:vm';
import { JSDOM } from 'jsdom';
import { randomUUID } from 'node:crypto';
import { installFixture } from './transport-fixture.mjs';

function event() { const listeners = []; return { listeners, addListener: fn => listeners.push(fn) }; }
function background(hostOffline = false) {
  const onMessage = event(); const local = {}; const sent = [];
  const runtime = { id: 'test', getURL: p => `chrome-extension://test/${p}`, onInstalled: event(), onMessage, lastError: undefined,
    connectNative: name => {
      assert.equal(name, 'io.mediaharbor.host');
      const incoming = event(); const closed = event();
      const port = { onMessage: incoming, onDisconnect: closed, disconnect() { for (const listener of closed.listeners) listener(); }, postMessage(request) {
        sent.push(request);
        queueMicrotask(() => {
          if (hostOffline) { runtime.lastError = { message: 'Not installed' }; port.disconnect(); }
          else for (const listener of incoming.listeners) listener({ version: 1, requestId: request.requestId, ok: true, data: { name: 'MediaHarbor' } });
        });
      } }; return port;
    }
  };
  const chrome = { runtime, storage: { local: { set: async values => Object.assign(local, values) } }, action: { openPopup: async () => {}, setBadgeText: async () => {}, setBadgeBackgroundColor: async () => {} }, contextMenus: { onClicked: event() }, tabs: { query: async () => [{ id: 1, url: 'https://open.spotify.com/playlist/test' }] } };
  return { chrome, local, sent, onMessage };
}
const script = await fs.readFile('apps/extension/.output/chrome-mv3/background.js', 'utf8');
function startBackground(offline) {
  const env = background(offline);
  vm.runInNewContext(script, { chrome: env.chrome, console, setTimeout, clearTimeout, queueMicrotask });
  return env;
}
function send(env, message, sender) { return new Promise(resolve => { const result = env.onMessage.listeners[0](message, sender, resolve); if (!result) resolve(undefined); }); }
const req = { version: 1, requestId: 'extension-test', op: 'ping' };
const env = startBackground(false);
assert.equal((await send(env, { type: 'rpc', request: req }, { url: 'chrome-extension://test/popup/index.html' })).ok, true);
assert.equal(env.sent.length, 1);
assert.equal(await send(env, { type: 'rpc', request: req }, { tab: { id: 1 }, url: 'https://www.youtube.com/' }), undefined);
assert.equal(env.sent.length, 1, 'content scripts must not access the native host directly');
await send(env, { type: 'page.open', source: 'https://www.youtube.com/watch?v=abcdefghijk' }, { tab: { id: 1 } });
assert.equal(env.local.pendingSource, 'https://www.youtube.com/watch?v=abcdefghijk');
assert.equal((await send(startBackground(true), { type: 'rpc', request: req }, { url: 'chrome-extension://test/popup/index.html' })).error.code, 'HOST_UNAVAILABLE');

for (const name of ['chrome', 'firefox']) {
  const manifest = JSON.parse(await fs.readFile(`apps/extension/.output/${name}-mv3/manifest.json`, 'utf8'));
  assert.equal(manifest.manifest_version, 3); assert.ok(manifest.permissions.includes('nativeMessaging'));
  if (name === 'firefox') { assert.equal(manifest.browser_specific_settings.gecko.id, 'mediaharbor@mediaharbor.local'); assert.equal(manifest.background.scripts[0], 'background.js'); }
  else { assert.equal(manifest.background.service_worker, 'background.js'); assert.ok(manifest.key); }
}

const dom = new JSDOM('<div id="root"></div>', { url: 'https://extension.test/popup/index.html', runScripts: 'dangerously', pretendToBeVisual: true });
try {
  const w = dom.window; w.crypto.randomUUID = randomUUID;
  w.eval(`(${installFixture.toString()})();`);
  const invoke = w.__TAURI__.core.invoke; delete w.__TAURI__;
  w.chrome = { runtime: { id: 'test', sendMessage: async message => invoke('rpc', { request: message.request }) }, storage: { local: { get: async () => ({}), remove: async () => {} }, onChanged: { addListener() {}, removeListener() {} } }, tabs: { query: async () => [{ url: 'https://www.youtube.com/watch?v=abcdefghijk' }] }, action: { setBadgeText: async () => {} } };
  w.eval(await fs.readFile('apps/extension/.output/chrome-mv3/popup/main.js', 'utf8'));
  async function wait(fn) { for (let i = 0; i < 200; i++) { if (fn()) return; await new Promise(r => setTimeout(r, 10)); } throw Error(`Popup timeout: ${w.document.body.textContent}`); }
  await wait(() => w.document.querySelector('input[aria-label="Ссылка или название"]'));
  const button = text => [...w.document.querySelectorAll('button')].find(b => b.textContent.includes(text));
  await wait(() => !button('Подставить ссылку').disabled);
  button('Подставить ссылку').click();
  await wait(() => w.document.querySelector('input[aria-label="Ссылка или название"]').value.includes('abcdefghijk'));
  w.document.querySelector('form').dispatchEvent(new w.Event('submit', { bubbles: true, cancelable: true }));
  await wait(() => button('Скачать выбранное') && !button('Скачать выбранное').disabled);
  button('Скачать выбранное').click();
  await wait(() => w.document.querySelector('.job') || w.document.body.textContent.includes('В очереди'));
} finally { dom.window.close(); }
console.log('Extension smoke passed: both manifests, native bridge, sender isolation, missing host, page transfer, popup → queue.');
