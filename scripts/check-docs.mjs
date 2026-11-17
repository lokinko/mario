// Validate the rebuilt, approved documentation and the current HTTP route inventory.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { resolve, relative, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { rustRoutes } from './check-architecture.mjs';
export { rustRoutes } from './check-architecture.mjs';

const root = resolve(import.meta.dirname, '..');
function files(directory, suffix) {
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? files(path, suffix) : path.endsWith(suffix) ? [path] : [];
  });
}
function headings(source) {
  const anchors = new Set(), duplicates = new Map();
  const text = source.replace(/^\s*(`{3,}|~{3,})[^\n]*\n[\s\S]*?^\s*\1\s*$/gm, '');
  for (const match of text.matchAll(/^#{1,6}\s+(.+?)\s*#*\s*$/gm)) {
    const base = match[1].replace(/`/g, '').toLowerCase().replace(/[^\p{L}\p{N}_\-\s]/gu, '').replace(/ /g, '-');
    const count = duplicates.get(base) ?? 0;
    duplicates.set(base, count + 1);
    anchors.add(base + (count ? `-${count}` : ''));
  }
  return anchors;
}
export function checkApiInventory(sourceFiles, markdown) {
  const actual = sourceFiles.flatMap(path => rustRoutes(readFileSync(path, 'utf8')));
  const documented = [...markdown.matchAll(/^\| (GET|POST|PUT|DELETE|PATCH|HEAD|OPTIONS) \| `([^`]+)` \|/gm)].map(match => `${match[1]} ${match[2]}`);
  assert.equal(new Set(actual).size, actual.length, 'duplicate production route');
  assert.equal(new Set(documented).size, documented.length, 'duplicate documented route');
  const missing = actual.filter(route => !documented.includes(route));
  const stale = documented.filter(route => !actual.includes(route));
  assert.deepEqual({ missing, stale }, { missing: [], stale: [] }, 'API inventory differs from production routes');
  return actual.length;
}
export function checkDocumentation(directory = root) {
  // Other retained files (e.g. evaluation fixture notes) need their own content review.
  const documents = ['README.md', 'AGENTS.md'].map(path => resolve(directory, path)).concat(files(resolve(directory, 'docs'), '.md'));
  const violations = [];
  let links = 0;
  for (const path of documents) {
    const source = readFileSync(path, 'utf8'), name = relative(directory, path);
    if (/^(?:<<<<<<<|=======|>>>>>>>)/m.test(source)) violations.push(`${name}: conflict marker`);
    for (const match of source.matchAll(/\]\((<[^>]+>|[^\s)]+)(?:\s+"[^"]*")?\)/g)) {
      const link = match[1].replace(/^<|>$/g, '');
      if (/^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(link)) continue;
      links++;
      const [target, fragment] = link.split('#');
      const destination = resolve(dirname(path), decodeURIComponent(target));
      if (!existsSync(destination)) { violations.push(`${name}: missing ${link}`); continue; }
      if (fragment && destination.endsWith('.md') && !headings(readFileSync(destination, 'utf8')).has(decodeURIComponent(fragment)))
        violations.push(`${name}: missing anchor ${link}`);
    }
  }
  assert.deepEqual(violations, [], `Documentation errors:\n${violations.join('\n')}`);
  const routeFiles = ['server/src/hosted.rs', 'server/src/agent.rs'].map(path => resolve(directory, path)).concat(files(resolve(directory, 'server/src/http'), '.rs'));
  const routes = checkApiInventory(routeFiles, readFileSync(resolve(directory, 'docs/api.md'), 'utf8'));
  return { documents: documents.length, links, routes };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const counts = checkDocumentation();
  console.log(`PASS docs: ${counts.documents} approved documents, ${counts.links} local references, ${counts.routes} API operations`);
}
