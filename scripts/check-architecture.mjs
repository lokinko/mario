// Source-level boundaries complement type checks and behavioral tests.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { resolve, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { storageOwners, storageOwner, tableOwners, sqlTableAccess, sqlStorageViolations, settingMethodOwners, storageMethodViolation } from './storage-boundaries.mjs';
import { serverLayers, serverModelOwners, serverApplicationOwners, serverPortOwners, serverHttpOwners, httpCapabilities, httpCapabilityViolation, httpRegistryViolations, httpOwnedRoutes, serverModelViolation, serverBusinessViolation, userAccountCapabilities, boundaryOf, clientBoundaryViolation, clientBusinessViolation, httpRouteViolation } from './architecture-boundaries.mjs';

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
  const businessViolation = clientBusinessViolation(source, target);
  if (businessViolation) return businessViolation;
  const from = clientLayer(source), to = clientLayer(target);
  if (from === 'features' && ['app', 'App.tsx', 'main.tsx'].includes(to))
    return 'features receive application callbacks, not application implementations';
  if (from === 'features' && to === 'features' && source.split('/')[1] !== target.split('/')[1])
    return 'features must compose through app or shared components';
  if (from === 'types' && to !== 'types') return 'contracts may only import other owned contracts';
  if (['lib', 'components', 'api', 'types'].includes(from) && ['features', 'app', 'App.tsx', 'main.tsx'].includes(to))
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
export function rustSource(source) {
  const masked = source.split(''), strings = [], excluded = [];
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
      const start = i;
      const close = '"' + raw[1];
      const end = source.indexOf(close, i + raw[0].length);
      strings.push({ start, value: source.slice(i + raw[0].length, end === -1 ? source.length : end) });
      hide(end === -1 ? source.length : end + close.length); continue;
    }
    if (source[i] === '"') {
      const start = i;
      let end = i + 1;
      for (; end < source.length; end++) {
        if (source[end] === '\\') end++;
        else if (source[end] === '"') { end++; break; }
      }
      strings.push({ start, value: source.slice(start + 1, end - 1).replace(/\\(["\\])/g, '$1') });
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
    excluded.push([match.index, end]);
    result = result.slice(0, match.index) + result.slice(match.index, end).replace(/[^\n]/g, ' ') + result.slice(end);
  }
  return { code: result, strings: strings.filter(({ start }) => !excluded.some(([a, b]) => start >= a && start < b)).map(({ value }) => value) };
}
export function productionRust(source) { return rustSource(source).code; }

export function databaseMethodNames(source) {
  const code = productionRust(source), names = new Set();
  for (const match of code.matchAll(/\bimpl\s+Database\s*\{/g)) {
    const start = match.index + match[0].length;
    let depth = 1, end = start;
    for (; end < code.length && depth; end++) { if (code[end] === '{') depth++; if (code[end] === '}') depth--; }
    const body = code.slice(start, end - 1);
    let level = 0, position = 0;
    for (const method of body.matchAll(/\bfn\s+(\w+)\s*\(/g)) {
      for (; position < method.index; position++) { if (body[position] === '{') level++; if (body[position] === '}') level--; }
      if (level === 0) names.add(method[1]);
    }
  }
  return names;
}
export function databaseCalls(source) {
  const code = productionRust(source), aliases = new Set(['Database', 'Self']);
  for (const match of code.matchAll(/\bDatabase\s+as\s+(\w+)/g)) aliases.add(match[1]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const match of code.matchAll(/\btype\s+(\w+)\s*=\s*(?:crate::db::)?(\w+)\s*;/g)) {
      if (aliases.has(match[2]) && !aliases.has(match[1])) { aliases.add(match[1]); changed = true; }
    }
  }
  const names = [...aliases].join('|');
  const associated = new RegExp(`\\b(?:${names})::(\\w+)`, 'g');
  const qualified = new RegExp(`<(?:crate::db::)?(?:${names})>\\s*::(\\w+)`, 'g');
  return new Set([
    ...[...code.matchAll(/\.\s*(\w+)\s*\(/g)].map(match => match[1]),
    ...[...code.matchAll(associated), ...code.matchAll(qualified)].map(match => match[1]),
  ]);
}

// Follow actual non-test module declarations instead of guessing from filenames.
export function productionModules(directory) {
  const modules = new Map();
  function visit(path, owner) {
    assert(!modules.has(path), `production module included twice: ${path}`);
    const source = readFileSync(path, 'utf8'), code = productionRust(source);
    assert(!/#\s*\[\s*path\s*=/.test(code), `extend production module resolution for #[path] in ${path}`);
    modules.set(path, owner);
    const base = /(?:^|\/)(?:lib|mod)\.rs$/.test(posix(path)) ? dirname(path) : path.slice(0, -3);
    for (const name of rustModules(source)) {
      const candidates = [resolve(base, name + '.rs'), resolve(base, name, 'mod.rs')].filter(existsSync);
      assert.equal(candidates.length, 1, `${path}: production module ${name} must resolve to exactly one source`);
      visit(candidates[0], [...owner, name]);
    }
  }
  visit(resolve(directory, 'lib.rs'), []);
  return modules;
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
// Resolve imported leaf symbols, including grouped trees, so a narrow boundary
// cannot be bypassed by importing its parent module under an alias.
export function rustPaths(source, modulePath = []) {
  let code = productionRust(source).replace(/\b(?:(?:super|self)::)+/g, value => {
    const owner = [...modulePath];
    for (const part of value.split('::')) if (part === 'super') owner.pop();
    return 'crate::' + (owner.length ? owner.join('::') + '::' : '');
  }).replace(/\bpub\s*\(\s*in\s+crate(?:::\w+)*\s*\)/g, value => value.replace(/[^\n]/g, ' '));
  const paths = new Set();
  code = code.replace(/\buse\s+([^;]+);/g, (statement, tree) => {
    const tokens = tree.match(/[A-Za-z_]\w*|::|[{},*]/g) ?? [];
    let index = 0;
    function branch(prefix = []) {
      const path = [...prefix];
      while (index < tokens.length) {
        const token = tokens[index++];
        if (token === '{') {
          while (index < tokens.length && tokens[index] !== '}') {
            if (tokens[index] === ',') { index++; continue; }
            branch(path);
          }
          index++;
          break;
        }
        if (token === '::') continue;
        if (token === ',' || token === '}') { index--; break; }
        if (token === 'as') { index++; break; }
        path.push(token);
        if (tokens[index] !== '::') break;
      }
      if (path[0] === 'crate' && tokens[index - 1] !== '}') paths.add(path.slice(1).join('::'));
      if (tokens[index] === 'as') index += 2;
    }
    branch();
    return statement.replace(/[^\n]/g, ' ');
  });
  for (const match of code.matchAll(/\bcrate::(\w+(?:::\w+)*)/g)) paths.add(match[1]);
  return paths;
}
export function serverSymbolViolation(source, target) {
  const httpViolation = httpCapabilityViolation(source, target);
  if (httpViolation) return httpViolation;
  const contractViolation = serverModelViolation(source, target);
  if (contractViolation) return contractViolation;
  const businessViolation = serverBusinessViolation(source, target);
  if (businessViolation) return businessViolation;
  if (['application/analysis.rs', 'ports/ai_account.rs', 'context.rs'].includes(source)
      && (['models::portfolio', 'models::portfolio::self'].includes(target)
        || ['models::portfolio::Snapshot', 'models::portfolio::HoldingTrade'].some(path => target === path || target.startsWith(path + '::'))))
    return 'AI inputs use the authorized current-state projection, never the full ledger snapshot';
  const capabilities = {
    'application/analysis.rs': ['AnalysisAccount', 'ModelConfiguration'],
    'application/daily_review.rs': ['DailyReviewAccount', 'ModelConfiguration'],
    'application/memory.rs': ['MemoryAccount'],
    'application/model_settings.rs': ['ModelConfiguration', 'ModelSettingsStore'],
    'application/codex_bridge.rs': ['ModelConfiguration', 'ModelSettingsStore'],
  }[source];
  if (capabilities && (target === 'ports::ai_account' || target.startsWith('ports::ai_account::'))
      && !capabilities.some(name => target === `ports::ai_account::${name}`))
    return 'AI use case may import only its declared account capabilities';
  const userCapabilities = userAccountCapabilities[source];
  if (userCapabilities && (target === 'ports::user_records' || target.startsWith('ports::user_records::')))
    return 'user use cases cannot widen their declared capability through record-management ports';
  if (capabilities && (target === 'ports::ai_records' || target.startsWith('ports::ai_records::')))
    return 'AI use cases cannot widen their declared capability through record-management ports';
  if (userCapabilities && (target === 'ports::user_account' || target.startsWith('ports::user_account::'))
      && !userCapabilities.some(name => target === `ports::user_account::${name}`))
    return 'user use case may import only its declared account capabilities';
  if (userCapabilities || ['ports/user_account.rs', 'db/user_access.rs'].includes(source)) {
    if (['ai', 'context', 'ports::ai_account', 'ports::analysis', 'ports::model',
      'application::analysis', 'application::memory', 'application::model_settings', 'application::codex_bridge']
      .some(path => target === path || target.startsWith(path + '::')))
      return 'user account workflows and adapters cannot obtain AI execution or account capabilities';
  }
  if (capabilities && (target === 'ports::user_account' || target.startsWith('ports::user_account::')))
    return 'AI use cases read user sources through their authorized input port, without ledger write capabilities';
  const narrow = {
    'application/dividend_plans.rs': ['db::PublicDividendPlans', 'models::market', 'ports::market::DividendPlanProvider', 'ports::market::DividendPlanReader', 'AppError', 'AppResult'],
    'application/dividend_calendar.rs': ['db::PublicWatch', 'models::watch::calendar', 'models::watch::WatchInput', 'ports::market::DividendPlanReader', 'AppResult'],
    'db/public_dividend_plans.rs': ['models::market', 'storage', 'AppError', 'AppResult'],
    'market_data/watch/plans.rs': ['models::market', 'market_data::watch::bad', 'AppError', 'AppResult'],
    'models/market/dividend_plans.rs': [],
    'ports/account.rs': ['AppResult'],
    'db/account_access.rs': ['db::Database', 'ports::account::AccountSession', 'AppResult'],
  }[source];
  if (narrow && !narrow.some(prefix => target === prefix || target.startsWith(prefix + '::')))
    return 'dependency exceeds this module capability; public plan acquisition and user projection stay separate';
}
export function rustRoutes(source) {
  const code = productionRust(source), routes = [];
  for (const match of code.matchAll(/\.route\s*\(/g)) {
    const start = match.index + match[0].length;
    const path = /^\s*"([^"\n]+)"\s*,/.exec(source.slice(start));
    assert(path, 'route path must be a string literal; extend the inventory scanner for new route syntax');
    let depth = 1, end = start;
    for (; end < code.length && depth; end++) {
      if (code[end] === '(') depth++;
      if (code[end] === ')') depth--;
    }
    assert.equal(depth, 0, `unbalanced route ${path[1]}`);
    if (path[1].includes('{*')) continue; // Unknown-path fallback is described in prose.
    const methods = [...code.slice(start, end - 1).matchAll(/\b(get|post|put|delete|patch|head|options)\s*\(/g)].map(item => item[1].toUpperCase());
    assert(methods.length, `unrecognized route methods: ${path[1]}`);
    routes.push(...methods.map(method => `${method} ${path[1]}`));
  }
  return routes;
}
export function rustModules(source) {
  return new Set([...productionRust(source).matchAll(/^\s*(?:pub(?:\([^)]*\))?\s+)?mod (\w+)\s*;/gm)].map(match => match[1]));
}
const core = new Set(serverLayers.domain);
const infrastructure = new Set(['db', 'storage', 'accounts', 'secrets', 'market_data']);
const transport = new Set(['http', 'hosted', 'runtime', 'agent', 'streaming', 'composition', 'codex_bridge_worker']);
export function serverViolation(source, target) {
  if (source === 'openai_login' && target !== 'error')
    return 'website OIDC verifies identity without account storage or model execution';
  if (target === 'openai_login' && source !== 'hosted')
    return 'website OIDC belongs to the hosted authentication adapter';
  if (source === 'chatgpt_auth' && target !== 'error')
    return 'local OAuth authentication cannot depend on server accounts, business state or AI execution';
  if (target === 'chatgpt_auth' && !['runtime', 'codex_bridge_worker', 'ai'].includes(source))
    return 'ChatGPT credentials belong only to the local authentication command and worker';
  if (source === 'secrets' && !['error'].includes(target))
    return 'credential encryption cannot depend on account persistence';
  if (source === 'ports' && !['models', 'context', 'error', 'ports'].includes(target))
    return 'execution contracts cannot depend on implementations or account state';
  if (source === 'market_data' && !['models', 'ports', 'error'].includes(target))
    return 'market adapters only implement query ports; account workflows belong in application';
  if (source === 'application' && target === 'market_data')
    return 'application receives market ports; concrete providers belong in composition';
  if (source === 'application' && target === 'ai')
    return 'model management and execution implementations belong behind injected ports';
  if (core.has(source) && target === 'ports') return 'domain rules cannot execute external ports';
  if (core.has(source) && (infrastructure.has(target) || transport.has(target) || ['application', 'ai'].includes(target)))
    return 'domain contracts and deterministic rules cannot depend on execution, persistence or transport';
  if (source === 'ai' && (infrastructure.has(target) || transport.has(target) || target === 'application'))
    return 'AI consumes frozen data and cannot depend on database or HTTP adapters';
  if (source === 'db' && ['application', 'ai', 'agent', 'http', 'hosted', 'runtime', 'fx', 'market_data'].includes(target))
    return 'repositories cannot depend on workflows, providers or network refresh services';
  if (source === 'application' && transport.has(target)) return 'use cases depend on execution ports, not process adapters';
  if (source === 'agent' && ['db', 'storage', 'accounts', 'application', 'http', 'hosted', 'runtime'].includes(target))
    return 'Agent must remain independent of account data and business HTTP handlers';
  return undefined;
}

export function serverFileViolation(source, target) {
  if (httpCapabilities[source] && ['db', 'storage', 'accounts'].includes(target))
    return 'business HTTP routes use sealed request contexts, never the unified account store';
  if (['application/analysis.rs', 'application/memory.rs', 'application/model_settings.rs', 'application/codex_bridge.rs'].includes(source)
      && ['db', 'storage', 'accounts'].includes(target))
    return 'AI use cases depend on narrow account ports, never the concrete database or ledger';
  if (userAccountCapabilities[source] && ['db', 'storage', 'accounts'].includes(target))
    return 'user use cases depend on declared account ports, never the unified database or arbitrary settings';
  if (source === 'ai/model_catalog.rs' && !['ports', 'error'].includes(target))
    return 'site model catalog only lists provider metadata, without account data or analysis context';
  if (source === 'accounts/activity.rs' && !['accounts', 'storage', 'error'].includes(target))
    return 'usage metadata belongs to account authority and cannot read tenant ledgers or execute workflows';
  if (['db/public_dividends.rs', 'db/public_watch.rs', 'db/public_dividend_plans.rs', 'db/public_prices.rs', 'db/public_prices/monitor.rs'].includes(source) && !['models', 'storage', 'error'].includes(target))
    return 'public market storage only depends on public contracts and the database driver boundary';
  if ((source === 'application/public_prices.rs' || source.startsWith('application/public_prices/')) && !['models', 'db', 'ports', 'error'].includes(target))
    return 'public price orchestration cannot depend on accounts, HTTP or concrete sources';
  if (['application/dividend_calendar.rs', 'application/dividend_plans.rs'].includes(source) && !['models', 'db', 'ports', 'error'].includes(target))
    return 'public dividend calendars cannot depend on accounts, private context or model execution';
  if (source === 'application/watch_catalog.rs' && !['models', 'db', 'ports', 'error', 'application'].includes(target))
    return 'public catalogue checks cannot depend on private context or execution adapters';
  if (source === 'market_data/watch_catalog.rs' && !['models', 'ports', 'error'].includes(target))
    return 'public catalogue source lookup cannot depend on accounts, storage or workflows';
  if (source === 'application/watch_market.rs' && !['models', 'db', 'ports', 'error'].includes(target))
    return 'shared public dividend lookup cannot depend on accounts, private context or execution adapters';
  if (target === 'chatgpt_auth' && source.startsWith('ai/') && source !== 'ai/chatgpt.rs')
    return 'only the local ChatGPT provider can read local OAuth credentials';
  if (source === 'application/analysis.rs' && target === 'ai')
    return 'analysis lifecycle depends on the execution port, not workflow implementations';
  return undefined;
}

export function checkArchitecture() {
  const violations = [], clientGraph = new Map(), serverGraph = new Map();
  violations.push(...httpRegistryViolations(serverHttpOwners, httpCapabilities, httpOwnedRoutes));
  const clientRoot = resolve(root, 'client/src');
  for (const path of files(clientRoot, /\.tsx?$/).filter(path => !/\.test\./.test(path) && !posix(path).includes('/test/'))) {
    const name = posix(relative(clientRoot, path));
    const boundaryViolation = clientBoundaryViolation(name);
    if (boundaryViolation) violations.push(`${name}: ${boundaryViolation}`);
    const source = readFileSync(path, 'utf8');
    const ast = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true);
    if (name.startsWith('types/') && ast.statements.some(statement =>
      !ts.isInterfaceDeclaration(statement) && !ts.isTypeAliasDeclaration(statement)
      && !(ts.isImportDeclaration(statement) && statement.importClause?.isTypeOnly)))
      violations.push(`${name}: contracts contain only type declarations and explicit type imports`);
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
  const production = productionModules(serverRoot);
  const storeMethods = new Map(Object.entries(settingMethodOwners)), storedTables = new Set();
  for (const [path] of production) {
    const name = posix(relative(serverRoot, path));
    if (name !== 'db.rs' && !name.startsWith('db/')) continue;
    for (const method of databaseMethodNames(readFileSync(path, 'utf8'))) {
      if (settingMethodOwners[method]) continue;
      if (storeMethods.has(method)) violations.push(`duplicate account method ownership: ${method}`);
      storeMethods.set(method, storageOwner(name));
    }
  }
  for (const { table, operation } of sqlTableAccess(readFileSync(resolve(root, 'server/migrations/tenant-revisions.sql'), 'utf8')))
    if (operation.startsWith('CREATE')) storedTables.add(table);
  const storedModules = rustModules(readFileSync(resolve(serverRoot, 'db.rs'), 'utf8'));
  for (const name of storedModules) if (!storageOwners[name]) violations.push(`db/${name}: unassigned storage owner`);
  for (const name of Object.keys(storageOwners)) {
    if (![...production.values()].some(path => path.join('/') === 'db/' + name)) violations.push(`db/${name}: stale storage owner registration`);
  }
  const modelSource = readFileSync(resolve(serverRoot, 'models.rs'), 'utf8');
  const modelModules = rustModules(modelSource);
  if (productionRust(modelSource).replace(/pub\(crate\)\s+mod\s+\w+\s*;/g, '').trim())
    violations.push('models.rs: contracts belong in explicitly owned modules; no shared definitions or reexports');
  for (const name of modelModules) {
    if (!serverModelOwners[name]) violations.push(`models/${name}: unassigned contract owner`);
  }
  for (const name of Object.keys(serverModelOwners)) {
    if (!modelModules.has(name)) violations.push(`models/${name}: stale contract owner registration`);
  }
  for (const [layer, owners] of [['application', serverApplicationOwners], ['ports', serverPortOwners], ['http', serverHttpOwners]]) {
    const source = readFileSync(resolve(serverRoot, layer, 'mod.rs'), 'utf8');
    const modules = rustModules(source);
    if (layer !== 'http' && productionRust(source).replace(/pub\(crate\)\s+mod\s+\w+\s*;/g, '').trim())
      violations.push(`${layer}/mod.rs: business entry points belong in explicitly owned modules`);
    for (const name of modules) if (!owners[name]) violations.push(`${layer}/${name}: unassigned business owner`);
    for (const name of Object.keys(owners)) if (!modules.has(name)) violations.push(`${layer}/${name}: stale business owner registration`);
  }
  const known = rustModules(readFileSync(resolve(serverRoot, 'lib.rs'), 'utf8'));
  const registrations = Object.values(serverLayers).flat();
  if (new Set(registrations).size !== registrations.length) violations.push('duplicate server boundary registration');
  for (const name of known) {
    if (!boundaryOf(serverLayers, name)) violations.push(`${name}: unassigned server boundary`);
  }
  for (const name of Object.values(serverLayers).flat()) {
    if (!known.has(name)) violations.push(`${name}: stale server boundary registration`);
  }
  for (const [path, modulePath] of production) {
    const name = posix(relative(serverRoot, path)), owner = modulePath[0];
    if (!known.has(owner)) continue;
    const source = readFileSync(path, 'utf8'), code = productionRust(source);
    if (owner === 'db') {
      for (const sql of rustSource(source).strings) {
        for (const { table, operation } of sqlTableAccess(sql)) if (operation.startsWith('CREATE')) storedTables.add(table);
        violations.push(...sqlStorageViolations(name, sql).map(message => `${name}: ${message}`));
      }
      for (const method of databaseCalls(source)) {
        const violation = storageMethodViolation(name, method, storeMethods.get(method));
        if (violation) violations.push(`${name}: ${violation}`);
      }
    }
    for (const target of rustPaths(source, modulePath)) {
      const violation = serverSymbolViolation(name, target);
      if (violation) violations.push(`${name} -> ${target}: ${violation}`);
    }
    if (owner === 'http') {
      for (const route of rustRoutes(source)) {
        const violation = httpRouteViolation(name, route);
        if (violation) violations.push(`${name}: ${route}: ${violation}`);
      }
    }
    if (['ports', 'application'].includes(owner) && /\breqwest::/.test(code))
      violations.push(`${name}: network clients belong in execution adapters`);
    if (['prices', 'market_data'].includes(owner) && /\bstatic\s+\w+\s*:/.test(code))
      violations.push(`${name}: mutable market resources belong to injected instances`);
    if (/use\s+(?:super|crate)::\*/.test(code)) violations.push(`${name}: production imports must be explicit`);
    if (owner !== 'storage' && /\b(?:postgres|rusqlite)::/.test(code)) violations.push(`${name}: database drivers belong in storage.rs`);
    if (['application', 'models', 'context', 'risk', 'planning', 'valuation', 'evidence', 'db', 'error'].includes(owner) && /\baxum::/.test(code))
      violations.push(`${name}: HTTP framework belongs in adapters`);
    if (!serverGraph.has(owner)) serverGraph.set(owner, new Set());
    for (const target of rustDependencies(source, modulePath)) {
      if (!known.has(target) || target === owner) continue;
      serverGraph.get(owner).add(target);
      const violation = serverViolation(owner, target) ?? serverFileViolation(name, target);
      if (violation) violations.push(`${name} -> ${target}: ${violation}`);
    }
  }
  for (const table of Object.keys(tableOwners)) if (!storedTables.has(table)) violations.push(`stale stored table ownership: ${table}`);
  violations.push(...cycles(clientGraph).map(c => `client cycle: ${c}`), ...cycles(serverGraph).map(c => `server cycle: ${c}`));
  assert.deepEqual(violations, [], `Architecture violations:\n${violations.join('\n')}`);
  return { clientModules: clientGraph.size, serverModules: serverGraph.size, serverFiles: production.size, storageTables: storedTables.size };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const counts = checkArchitecture();
  console.log(`PASS architecture: ${counts.clientModules} client files, ${counts.serverFiles} production Rust files (${counts.serverModules} modules), ${counts.storageTables} owned tables; boundaries and cycles checked`);
}
