import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('./build-installers.py', import.meta.url));
const candidates = process.platform === 'win32'
  ? [['py', ['-3']], ['python', []]]
  : [['python3', []]];
for (const [command, prefix] of candidates) {
  const result = spawnSync(command, [...prefix, script, ...process.argv.slice(2)], { stdio: 'inherit' });
  if (result.error?.code === 'ENOENT') continue;
  if (result.error) console.error(result.error.message);
  process.exit(result.status ?? 1);
}
console.error('Не найден Python 3.11+. Установите Python и повторите команду.');
process.exit(1);
