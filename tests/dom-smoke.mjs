import { build } from 'esbuild';
import { JSDOM, VirtualConsole } from 'jsdom';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
import { installFixture } from './transport-fixture.mjs';

const compiled = await build({ entryPoints: ['apps/desktop/src/main.tsx'], bundle: true, write: false, format: 'iife', loader: { '.css': 'empty' }, define: { 'process.env.NODE_ENV': '"production"' } });
const errors = [];
const console = new VirtualConsole();
console.on('jsdomError', error => errors.push(error.message));
const dom = new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>', { url: 'http://mediaharbor.test/', runScripts: 'dangerously', pretendToBeVisual: true, virtualConsole: console });
const window = dom.window; const document = window.document;
window.crypto.randomUUID = randomUUID;
window.eval(`(${installFixture.toString()})();`);
window.eval(compiled.outputFiles[0].text);
async function wait(predicate) {
  for (let i = 0; i < 200; i++) { if (predicate()) return; await new Promise(resolve => setTimeout(resolve, 10)); }
  throw new Error(`UI condition timed out: ${predicate.toString()}\n${document.body.textContent}`);
}
function button(text) { const value = [...document.querySelectorAll('button')].find(b => b.textContent.includes(text)); assert.ok(value, `Missing button: ${text}`); return value; }
function fill(element, value) {
  const prototype = element.tagName === 'SELECT' ? window.HTMLSelectElement.prototype : window.HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, value);
  element.dispatchEvent(new window.Event(element.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true }));
}
try {
  await wait(() => document.querySelector('h2')?.textContent === 'Что сохраним сегодня?');
  fill(document.querySelector('input[aria-label="Ссылка или название"]'), 'Artist — Song');
  document.querySelector('form').dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true }));
  await wait(() => document.querySelector('button.candidate'));
  document.querySelector('button.candidate').click();
  await wait(() => !button('Скачать выбранное').disabled);
  button('Скачать выбранное').click();
  await wait(() => document.body.textContent.includes('Добавлено задач: 1'));
  button('Очередь').click();
  await wait(() => document.querySelectorAll('.job').length === 1);
  button('Сжатие').click(); await wait(() => document.querySelector('.dropzone'));
  document.querySelector('.dropzone').click();
  await wait(() => document.body.textContent.includes('tone.wav'));
  button('Начать сжатие').click();
  await wait(() => document.querySelectorAll('.job').length === 2);
  button('Настройки').click(); await wait(() => document.body.textContent.includes('Сохранение и оформление'));
  const theme = [...document.querySelectorAll('select')].find(s => [...s.options].some(o => o.value === 'light'));
  fill(theme, 'light'); button('Сохранить настройки').click();
  await wait(() => document.documentElement.dataset.theme === 'light');
  assert.deepEqual(errors, []);
  globalThis.console.log('DOM smoke passed: search → confirm → queue; pick files → compress → queue; save theme. Transport mocked, real React UI.');
} finally { window.close(); }
