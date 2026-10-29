// Offline-capable packaging of the same WXT entrypoints. WXT remains the primary
// dev/release toolchain; this small adapter supports only the primitives we use.
import { build } from 'esbuild';
import fs from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const root = path.resolve('apps/extension');
const target = process.argv[2];
const targets = target ? [target] : ['chrome', 'firefox'];
if (targets.some(t => !['chrome', 'firefox'].includes(t))) throw Error('Use chrome or firefox');
const configuration = await build({
  entryPoints: [path.join(root, 'wxt.config.ts')], platform: 'node', format: 'esm', bundle: true, write: false,
  tsconfig: path.resolve('tsconfig.json'),
  define: { 'import.meta.url': JSON.stringify(pathToFileURL(path.join(root, 'wxt.config.ts')).href) },
  plugins: [{ name: 'wxt-config', setup(api) {
    api.onResolve({ filter: /^wxt$/ }, () => ({ path: 'wxt', namespace: 'configuration' }));
    api.onLoad({ filter: /.*/, namespace: 'configuration' }, () => ({ contents: 'export const defineConfig = value => value;' }));
  } }]
});
const config = (await import(`data:text/javascript;base64,${Buffer.from(configuration.outputFiles[0].text).toString('base64')}`)).default;
const browserPlugin = { name: 'browser-api', setup(api) {
  api.onResolve({ filter: /^wxt\/browser$/ }, () => ({ path: 'browser', namespace: 'runtime' }));
  api.onLoad({ filter: /.*/, namespace: 'runtime' }, () => ({ contents: 'export const browser = globalThis.browser?.runtime?.id ? globalThis.browser : globalThis.chrome;' }));
} };
for (const browser of targets) {
  const output = path.join(root, '.output', `${browser}-mv3`);
  await fs.mkdir(path.join(output, 'popup'), { recursive: true });
  await fs.cp(path.join(root, 'public'), output, { recursive: true });
  const shared = { bundle: true, format: 'iife', tsconfig: path.resolve('tsconfig.json'), target: browser === 'chrome' ? 'chrome120' : 'firefox140', plugins: [browserPlugin], minify: true, define: { 'process.env.NODE_ENV': '"production"' } };
  await build({ ...shared, entryPoints: [path.join(root, 'entrypoints/background.ts')], outfile: path.join(output, 'background.js'), banner: { js: 'const defineBackground = fn => fn();' } });
  await build({ ...shared, entryPoints: [path.join(root, 'entrypoints/media.content.ts')], outfile: path.join(output, 'media.content.js'), banner: { js: 'const defineContentScript = def => def.main({ onInvalidated(fn) { addEventListener("pagehide", e => { if (!e.persisted) fn(); }); } });' } });
  await build({ ...shared, entryPoints: [path.join(root, 'entrypoints/popup/main.tsx')], outfile: path.join(output, 'popup/main.js') });
  await fs.writeFile(path.join(output, 'popup/index.html'), '<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>MediaHarbor</title><link rel="stylesheet" href="main.css"></head><body class="extension-body"><div id="root"></div><script src="main.js"></script></body></html>');
  const manifest = await config.manifest({ browser, manifestVersion: 3, mode: 'production', command: 'build' });
  manifest.manifest_version = 3;
  manifest.action.default_popup = 'popup/index.html';
  manifest.background = browser === 'firefox' ? { scripts: ['background.js'] } : { service_worker: 'background.js' };
  manifest.content_scripts = [{ matches: manifest.host_permissions, js: ['media.content.js'], run_at: 'document_idle' }];
  manifest.icons = Object.fromEntries(Object.entries(manifest.icons).map(([size, name]) => [size, name.replace(/^\//, '')]));
  await fs.writeFile(path.join(output, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
  console.log(`Built ${browser}: ${output}`);
}
