// Exercises the production React UI against an explicit Tauri transport fixture.
// This does not substitute for native installer / real-service acceptance tests.
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { installFixture } from './transport-fixture.mjs';

const root = path.resolve('apps/desktop/dist');
await fs.access(path.join(root, 'index.html'));
const executablePath = process.env.MEDIAHARBOR_TEST_CHROME;
const browser = await chromium.launch({ headless: true, executablePath, args: ['--no-sandbox'] });
try {
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.route('http://mediaharbor.test/**', async route => {
    const url = new URL(route.request().url());
    const relative = url.pathname === '/' ? 'index.html' : url.pathname.slice(1);
    const file = path.resolve(root, relative);
    if (!file.startsWith(root + path.sep)) return route.fulfill({ status: 403, body: '' });
    await route.fulfill({ path: file });
  });
  await page.addInitScript(installFixture);
  await page.goto('http://mediaharbor.test/');
  await page.getByRole('heading', { name: 'Что сохраним сегодня?' }).waitFor();
  await page.getByLabel('Ссылка или название').fill('Artist — Song');
  await page.getByRole('button', { name: /Найти ↗/ }).click();
  await page.getByRole('button', { name: /Artist — Song.*3:42/ }).click();
  await page.getByRole('button', { name: '↓ Скачать выбранное' }).click();
  await page.getByRole('status').filter({ hasText: 'Добавлено задач: 1' }).waitFor();
  await page.getByRole('button', { name: /≋Очередь/ }).click();
  await page.getByText('Artist — Song', { exact: true }).waitFor();
  await page.getByRole('button', { name: /◫Сжатие/ }).click();
  await page.getByRole('button', { name: /Перетащите аудио или видео/ }).click();
  await page.getByText('tone.wav', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'Начать сжатие' }).click();
  await page.getByRole('heading', { name: 'Очередь', exact: true }).waitFor();
  await page.getByRole('button', { name: /⚙Настройки/ }).click();
  await page.getByLabel('Тема', { exact: true }).selectOption('light');
  await page.getByRole('button', { name: 'Сохранить настройки' }).click();
  await page.waitForFunction(() => document.documentElement.dataset.theme === 'light');
  await page.getByRole('button', { name: /↓Загрузить/ }).click();
  await page.screenshot({ path: '/tmp/mediaharbor-light.png', fullPage: true });
  await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
  await page.screenshot({ path: '/tmp/mediaharbor-dark.png', fullPage: true });
  assert.deepEqual(errors, []);
  console.log('UI smoke passed: search, source confirmation, queue, local files, compression submission, theme; no page errors.');
} finally { await browser.close(); }
