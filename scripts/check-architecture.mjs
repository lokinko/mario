// Source-level boundaries complement type checks and behavioral tests.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const ts = createRequire(resolve(root, 'client/package.json'))('typescript');
const posix = value => value.replaceAll('\\', '/');
function files(directory, extension) {
  return readdirSync(directory, { withFileTypes: true }).flatMap(entry => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? files(path, extension) : extension.test(path) ? [path] : [];
  });
}
function clientLayer(path) {
  return path.split('/')[0];
}
export function clientViolation(source, target) {
  const from = clientLayer(source), to = clientLayer(target);
  if (from === 'features' && ['app', 'App.tsx', 'main.tsx'].includes(to))
    return 'features receive application callbacks, not application implementations';
  if (from === 'features' && to === 'features' && source.split('/')[1] !== target.split('/')[1])
    return 'features must compose through app or shared components';
  if (['lib', 'components', 'api', 'types.ts'].includes(from) && ['features', 'app', 'App.tsx', 'main.tsx'].includes(to))
    return 'shared modules cannot depend on application pages';
  if (from === 'lib' && to === 'api') return 'transport and sync primitives cannot import domain API clients';
  if (from === 'api' && to === 'api' && source.split('/')[1].replace(/\.tsx?$/, '') !== target.split('/')[1].replace(/\.tsx?$/, '')) return 'domain clients cannot import another domain client';
  return undefined;
}
export function cycles(graph) {
  const active = [], done = new Set(), found = [];
  function visit(node) {
    const start = active.indexOf(node);
    if (start !== -1) { found.push([...active.slice(start), node].join(' -> ')); return; }
    if (done.has(node)) return;
    active.push(node);
    for (const target of graph.get(node) ?? []) visit(target);
    active.pop(); done.add(node);
  }
  for (const node of graph.keys()) visit(node);
  return found;
}

// Mask strings and comments before looking for Rust paths; raw prompt strings are
// data and test-only modules are not production dependencies. Preserve offsets.
export function productionRust(source) {
  const masked = source.split('');
  let i = 0;
  const hide = end => { for (; i < end; i++) if (masked[i] !== '\n') masked[i] = ' '; };
  while (i < source.length) {
    if (source.startsWith('//', i)) {
      const end = source.indexOf('\n', i); hide(end === -1 ? source.length : end); continue;
    }
    if (source.startsWith('/*', i)) {
      let depth = 1, end = i + 2;
      while (end < source.length && depth) {
        if (source.startsWith('/*', end)) { depth++; end += 2; }
        else if (source.startsWith('*/', end)) { depth--; end += 2; }
        else end++;
      }
      hide(end); continue;
    }
    const raw = /^(?:br|r)(#*)"/.exec(source.slice(i));
    if (raw) {
      const close = '"' + raw[1];
      const end = source.indexOf(close, i + raw[0].length);
      hide(end === -1 ? source.length : end + close.length); continue;
    }
    if (source[i] === '"') {
      let end = i + 1;
      for (; end < source.length; end++) {
        if (source[end] === '\\') end++;
        else if (source[end] === '"') { end++; break; }
      }
      hide(end); continue;
    }
    // Character literals, including braces, must not affect item balancing.
    const character = /^'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^'\n\\])'/.exec(source.slice(i));
    if (character) { hide(i + character[0].length); continue; }
    i++;
  }
  let result = masked.join('');
  for (const match of result.matchAll(/#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/g)) {
    let end = match.index + match[0].length;
    while (end < result.length && !['{', ';'].includes(result[end])) end++;
    if (result[end] === '{') {
      let depth = 1; end++;
      while (end < result.length && depth) { if (result[end] === '{') depth++; if (result[end] === '}') depth--; end++; }
    } else end++;
    result = result.slice(0, match.index) + result.slice(match.index, end).replace(/[^\n]/g, ' ') + result.slice(end);
  }
  return result;
}
export function rustDependencies(source, modulePath = []) {
  // Resolve relative paths before inspecting imports, so super::db cannot hide
  // the same dependency that crate::db would expose. Test items are masked first.
  const code = productionRust(source).replace(/\b(?:(?:super|self)::)+/g, path => {
    const owner = [...modulePath];
    for (const segment of path.split('::').filter(Boolean)) {
      if (segment === 'super') owner.pop();
    }
    return 'crate::' + (owner.length ? owner.join('::') + '::' : '');
  }), names = new Set();
  for (const match of code.matchAll(/\bcrate::(\w+)/g)) names.add(match[1]);
  for (const match of code.matchAll(/\bcrate::\{/g)) {
    let i = match.index + match[0].length, depth = 0, field = '';
    const add = () => { const name = /^\s*(\w+)/.exec(field)?.[1]; if (name) names.add(name); field = ''; };
    for (; i < code.length; i++) {
      const c = code[i];
      if (c === '}' && depth === 0) { add(); break; }
      if (c === ',' && depth === 0) { add(); continue; }
      if (depth === 0) field += c;
      if (c === '{') depth++;
      if (c === '}') depth--;
    }
  }
  return names;
}
const core = new Set(['models', 'context', 'risk', 'planning', 'valuation', 'evidence', 'event_import']);
const infrastructure = new Set(['db', 'storage', 'accounts', 'secrets', 'prices', 'market_data']);
const transport = new Set(['http', 'hosted', 'runtime', 'agent', 'streaming', 'composition', 'codex_bridge_worker']);
export function serverViolation(source, target) {
  if (source === 'secrets' && !['error'].includes(target))
    return 'credential encryption cannot depend on account persistence';
  if (source === 'ports' && !['models', 'context', 'error', 'ports'].includes(target))
    return 'execution contracts cannot depend on implementations or account state';
  if (['prices', 'market_data'].includes(source) && !['models', 'ports', 'error'].includes(target))
    return 'market adapters only implement query ports; account workflows belong in application';
  if (source === 'application' && ['prices', 'market_data'].includes(target))
    return 'application receives market ports; concrete providers belong in composition';
  if (core.has(source) && target === 'ports') return 'domain rules cannot execute external ports';
  if (core.has(source) && (infrastructure.has(target) || transport.has(target) || ['application', 'ai'].includes(target)))
    return 'domain contracts and deterministic rules cannot depend on execution, persistence or transport';
  if (source === 'ai' && (infrastructure.has(target) || transport.has(target) || target === 'application'))
    return 'AI consumes frozen data and cannot depend on database or HTTP adapters';
  if (source === 'db' && ['application', 'ai', 'agent', 'http', 'hosted', 'runtime', 'fx', 'prices', 'market_data'].includes(target))
    return 'repositories cannot depend on workflows, providers or network refresh services';
  if (source === 'application' && transport.has(target)) return 'use cases depend on execution ports, not process adapters';
  if (source === 'agent' && ['db', 'storage', 'accounts', 'application', 'http', 'hosted', 'runtime'].includes(target))
    return 'Agent must remain independent of account data and business HTTP handlers';
  return undefined;
}

export function serverFileViolation(source, target) {
  if (source === 'application/analysis.rs' && target === 'ai')
    return 'analysis lifecycle depends on the execution port, not workflow implementations';
  return undefined;
}

export function checkArchitecture() {
  const violations = [], clientGraph = new Map(), serverGraph = new Map();
  const clientRoot = resolve(root, 'client/src');
  for (const path of files(clientRoot, /\.tsx?$/).filter(path => !/\.test\./.test(path) && !posix(path).includes('/test/'))) {
    const name = posix(relative(clientRoot, path));
    const source = readFileSync(path, 'utf8');
    const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
    const dependencies = new Set();
    const inspect = node => {
      let specifier;
      if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) specifier = node.moduleSpecifier;
      if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) specifier = node.argument.literal;
      if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) specifier = node.arguments[0];
      if (specifier && ts.isStringLiteral(specifier) && specifier.text.startsWith('.')) {
        if (/\.(?:css|svg|json)$/.test(specifier.text)) {
          if (!existsSync(resolve(dirname(path), specifier.text))) violations.push(`${name}: unresolved asset ${specifier.text}`);
          ts.forEachChild(node, inspect);
          return;
        }
        const base = resolve(dirname(path), specifier.text);
        const targetPath = [base, base + '.ts', base + '.tsx', resolve(base, 'index.ts'), resolve(base, 'index.tsx')].find(p => /\.tsx?$/.test(p) && existsSync(p));
        if (!targetPath) violations.push(`${name}: unresolved import ${specifier.text}`);
        else {
          const target = posix(relative(clientRoot, targetPath));
          dependencies.add(target);
          const violation = clientViolation(name, target);
          if (violation) violations.push(`${name} -> ${target}: ${violation}`);
        }
      }
      ts.forEachChild(node, inspect);
    };
    inspect(ast); clientGraph.set(name, dependencies);
  }
  const reachable = new Set();
  const visitClient = name => {
    if (reachable.has(name)) return;
    reachable.add(name);
    for (const dependency of clientGraph.get(name) ?? []) visitClient(dependency);
  };
  visitClient('main.tsx');
  for (const name of clientGraph.keys()) {
    if (!reachable.has(name)) violations.push(`${name}: unused module, not reachable from main.tsx`);
  }
  const serverRoot = resolve(root, 'server/src');
  const known = new Set([...readFileSync(resolve(serverRoot, 'lib.rs'), 'utf8').matchAll(/^mod (\w+);/gm)].map(m => m[1]));
  for (const path of files(serverRoot, /\.rs$/).filter(path => !/(?:^|[\\/])(?:tests|postgres_tests|sync_merge)\.rs$/.test(path))) {
    const name = posix(relative(serverRoot, path)), owner = name.split('/')[0].replace(/\.rs$/, '');
    if (!known.has(owner)) continue;
    const source = readFileSync(path, 'utf8'), code = productionRust(source);
    if (['ports', 'application'].includes(owner) && /\breqwest::/.test(code))
      violations.push(`${name}: network clients belong in execution adapters`);
    if (['prices', 'market_data'].includes(owner) && /\bstatic\s+\w+\s*:/.test(code))
      violations.push(`${name}: mutable market resources belong to injected instances`);
    if (owner !== 'db' && /use\s+(?:super|crate)::\*/.test(code)) violations.push(`${name}: production imports must be explicit`);
    if (['application', 'models', 'context', 'risk', 'planning', 'valuation', 'evidence', 'event_import', 'db', 'error'].includes(owner) && /\baxum::/.test(code))
      violations.push(`${name}: HTTP framework belongs in adapters`);
    if (!serverGraph.has(owner)) serverGraph.set(owner, new Set());
    for (const target of rustDependencies(source, name.replace(/\.rs$/, '').replace(/\/mod$/, '').split('/'))) {
      if (!known.has(target) || target === owner) continue;
      serverGraph.get(owner).add(target);
      const violation = serverViolation(owner, target) ?? serverFileViolation(name, target);
      if (violation) violations.push(`${name} -> ${target}: ${violation}`);
    }
  }
  violations.push(...cycles(clientGraph).map(c => `client cycle: ${c}`), ...cycles(serverGraph).map(c => `server cycle: ${c}`));
  assert.deepEqual(violations, [], `Architecture violations:\n${violations.join('\n')}`);
  return { clientModules: clientGraph.size, serverModules: serverGraph.size };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const counts = checkArchitecture();
  console.log(`PASS architecture: ${counts.clientModules} client files, ${counts.serverModules} server modules; boundaries and cycles checked`);
}
