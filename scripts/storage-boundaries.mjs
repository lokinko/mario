// Account ownership is independent from the physical schema and shared transaction.
export const storageOwners = {
  account_access: 'assembly', settings: 'assembly', schema: 'assembly',
  sync_format: 'assembly', sync_store: 'assembly', sync_validation: 'assembly', watchlist_import: 'assembly',
  'settings/user': 'user', 'settings/ai': 'ai',
  amounts: 'user', cash: 'user', corporate: 'user', daily: 'user', evidence: 'user', financial: 'user',
  fx: 'user', methodology: 'user', options: 'user', portfolio: 'user', public_watch: 'user',
  reminders: 'user', reviews: 'user', rule_values: 'user', snapshot: 'user', trades: 'user',
  user_access: 'user', user_records: 'user', validation: 'user', watch: 'user', watch_grid_migration: 'user',
  ai_access: 'ai', ai_records: 'ai', analysis_history: 'ai', daily_review: 'ai', memory: 'ai', model_settings: 'ai', secrets: 'ai',
  public_announcements: 'market', public_prices: 'market', public_dividends: 'market', public_dividend_plans: 'market',
};
export function storageOwner(source) {
  if (source === 'db.rs') return 'assembly';
  if (!source.startsWith('db/')) return;
  const module = source.slice(3).replace(/\.rs$/, '');
  const name = Object.keys(storageOwners).sort((a, b) => b.length - a.length)
    .find(name => module === name || module.startsWith(name + '/'));
  return name ? storageOwners[name] : undefined;
}

export const tableOwners = Object.fromEntries(Object.entries({
  user: ['cash_state', 'corporate_actions', 'daily_entries', 'daily_settings', 'daily_totals',
    'financial_journal', 'financial_operations', 'holding_amounts', 'holding_options', 'holding_trades',
    'holding_valuations', 'holdings', 'investment_rule_revisions', 'investment_rules', 'profile',
    'research_attachments', 'research_evidence', 'system_reviews', 'trade_settlements', 'trading_quarantine',
    'watch_catalog', 'watch_dividends', 'watch_entries', 'watch_observations', 'watch_snapshots'],
  ai: ['analyses', 'analysis_memories', 'daily_review_items', 'daily_review_runs'],
  market: ['announcement_snapshots', 'dividend_histories', 'dividend_plan_snapshots', 'price_fetch_days', 'price_histories', 'price_monitor_days', 'price_universe'],
  shared: ['settings', 'data_revision'],
}).flatMap(([owner, names]) => names.map(name => [name, owner])));

// Shared metadata and immutable transport validation are the only cross-store helpers.
export const sharedStorageImports = new Set([
  'db::Database', 'db::AccountLock',
  'db::sync_format::SyncDataset', 'db::sync_format::SyncTable', 'db::sync_format::SyncValue',
  'db::sync_validation::validate_synced_fx_provenance', 'db::sync_validation::validate_synced_holding_valuations',
]);
export function storageImportViolation(source, target) {
  const owner = storageOwner(source);
  if (!['user', 'ai', 'market'].includes(owner) || !(target === 'db' || target.startsWith('db::'))) return;
  if (sharedStorageImports.has(target)) return;
  const parts = target.split('::').slice(1);
  if (parts[0] === 'Database' || parts[0] === 'Self') return; // Calls are checked by declared method ownership.
  const targetOwner = ['PublicAnnouncements', 'PublicPrices', 'PublicDividends', 'PublicDividendPlans'].includes(parts[0]) ? 'market'
    : parts[0] === 'PublicWatch' ? 'user' : storageOwner('db/' + parts.join('/') + '.rs');
  if (!targetOwner || targetOwner === 'assembly' || targetOwner !== owner)
    return 'owned repositories import only their domain helpers or explicit shared metadata and validation';
}

export const settingsSqlFiles = new Set(['db/fx.rs', 'db/watch/rules.rs', 'db/memory.rs', 'db/model_settings.rs']);
const transactionCoordinators = new Set(['db.rs', 'db/schema.rs', 'db/sync_store.rs']);
export function sqlTableAccess(sql) {
  const accesses = [];
  const identifier = String.raw`(?:[a-z_]\w*|"[a-z_]\w*")(?:\.(?:[a-z_]\w*|"[a-z_]\w*"))?`;
  const pattern = new RegExp(String.raw`\b(INSERT\s+INTO|UPDATE(?!\s+SET\b)|DELETE\s+FROM|FROM|JOIN|REFERENCES|CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?|ALTER\s+TABLE|DROP\s+TABLE(?:\s+IF\s+EXISTS)?)\s+(${identifier})`, 'gi');
  const code = sql.replace(/'(?:''|[^'])*'|--[^\n]*|\/\*[\s\S]*?\*\//g, ' ')
    .replace(/\bIS\s+(?:NOT\s+)?DISTINCT\s+FROM\b/gi, ' IS DISTINCT ')
    .replace(/\b(FROM|JOIN)\s+LATERAL\b/gi, '$1');
  if (!/^\s*(?:SELECT|INSERT|UPDATE|DELETE|CREATE|ALTER|DROP|WITH|DO|SET)\b/i.test(code)) return accesses;
  for (const match of code.matchAll(pattern)) {
    const operation = match[1].replace(/\s+/g, ' ').toUpperCase();
    if (['FROM', 'JOIN'].includes(operation) && /^\s*\(/.test(code.slice(match.index + match[0].length))) continue;
    accesses.push({ table: match[2].replaceAll('"', '').toLowerCase(), operation });
  }
  return accesses;
}
export function sqlStorageViolations(source, sql) {
  const owner = storageOwner(source), issues = [];
  for (const { table, operation } of sqlTableAccess(sql)) {
    if (table.startsWith('pg_') || table.startsWith('information_schema.')) {
      if (!['FROM', 'JOIN'].includes(operation)) issues.push('system catalog writes belong in the storage driver');
      continue;
    }
    if (operation.startsWith('CREATE') && !tableOwners[table]) issues.push(`unassigned stored table ${table}`);
    if (owner === 'assembly' && tableOwners[table] && tableOwners[table] !== 'shared' && !transactionCoordinators.has(source))
      issues.push('cross-domain table coordination belongs only in schema migration or backup transactions');
    if (!['user', 'ai', 'market'].includes(owner)) continue; // Migrations and backup coordinate registered owners.
    if (!tableOwners[table]) issues.push(`unknown stored relation ${table}`);
    else if (!['shared', owner].includes(tableOwners[table])) issues.push(`${operation} ${table} belongs to ${tableOwners[table]}, not ${owner}`);
    if (table === 'settings') {
      if (!settingsSqlFiles.has(source)) issues.push('settings SQL belongs in typed settings storage');
      for (const match of sql.matchAll(/\b(?:key\s*=|VALUES\s*\()\s*'([^']+)'/gi)) {
        const key = match[1];
        const keyOwner = /^(?:fx\.(?:book|quote)\.|investment\.methodology$|watch\.rules$)/.test(key) ? 'user'
          : /^(?:model\.|secret\.model$|analysis\.settings$|memory\.dream_state$|daily_review\.settings$)/.test(key) ? 'ai' : undefined;
        if (keyOwner !== owner) issues.push(`setting key ${key} does not belong to ${owner}`);
      }
    }
  }
  if (['user', 'ai', 'market'].includes(owner) && /\b(?:INSERT\s+INTO|UPDATE|DELETE\s+FROM|FROM|JOIN)\s+\{/i.test(sql))
    issues.push('dynamic business table names belong only in migration or backup coordination');
  return [...new Set(issues)];
}

export const settingMethodOwners = {
  user_setting: 'user', set_user_setting: 'user', ai_setting: 'ai', set_ai_setting: 'ai', delete_ai_setting: 'ai',
  export_sync_data: 'backup', import_sync_data: 'backup', import_account_data: 'backup', is_empty_for_import: 'backup',
};
const aiUserReads = new Set(['snapshot', 'investment_methodology', 'research_evidence', 'watch_board', 'daily_history', 'investment_rules']);
export function storageMethodViolation(source, name, targetOwner) {
  const owner = storageOwner(source);
  if (!['user', 'ai'].includes(owner) || !targetOwner || targetOwner === 'assembly') return;
  if (owner === targetOwner) return;
  if (source === 'db/ai_access.rs' && targetOwner === 'user' && aiUserReads.has(name)) return;
  return `account operation ${name} belongs to ${targetOwner}; cross-domain access needs an explicit owned interface`;
}
