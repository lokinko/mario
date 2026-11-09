import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(import.meta.dirname, '..');

export function checkEntrypoints(directory = root) {
  const scripts = JSON.parse(readFileSync(resolve(directory, 'package.json'), 'utf8')).scripts;
  const commands = [...Object.values(scripts), readFileSync(resolve(directory, 'scripts/test.sh'), 'utf8')];
  const paths = new Set(commands.flatMap(command => [...command.matchAll(/\bscripts\/[A-Za-z0-9_.-]+\.(?:mjs|sh|py)\b/g)].map(match => match[0])));
  const missing = [...paths].filter(path => !existsSync(resolve(directory, path)));
  assert.deepEqual(missing, [], `验证入口引用不存在的文件：${missing.join(', ')}`);
  return paths.size;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  console.log(`PASS entrypoints: ${checkEntrypoints()} script targets exist`);
