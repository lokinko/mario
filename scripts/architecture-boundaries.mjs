// Technical registrations; business ownership and data authority: docs/architecture.md.
// New top-level modules must be assigned deliberately before they enter the graph.
import { storageOwner, storageImportViolation } from './storage-boundaries.mjs';
export const serverLayers = {
  domain: ['models', 'context', 'risk', 'planning', 'valuation', 'evidence'],
  ports: ['ports'],
  application: ['application'],
  persistence: ['db', 'storage', 'accounts', 'secrets'],
  providers: ['market_data', 'ai', 'attachments', 'authentication', 'chatgpt_auth', 'openai_login'],
  adapters: ['http', 'hosted', 'agent', 'streaming', 'codex_bridge_worker'],
  composition: ['composition', 'runtime'],
  shared: ['error'],
};
export const clientLayers = {
  composition: ['main.tsx', 'App.tsx', 'app'],
  features: ['features'],
  api: ['api'],
  shared: ['components', 'lib', 'types'],
};
export const featureModules = ['account', 'dailyReview', 'dashboard', 'methodology', 'portfolio', 'advisor', 'evidence', 'reviews', 'settings', 'watch'];
export const apiModules = ['account', 'activity', 'dailyReview', 'memory', 'methodology', 'portfolio', 'analysis', 'evidence', 'reviews', 'settings', 'watch'];
export function boundaryOf(layers, name) {
  return Object.entries(layers).find(([, modules]) => modules.includes(name))?.[0];
}
export function clientBoundaryViolation(path) {
  const [top, domain] = path.split('/');
  if (!boundaryOf(clientLayers, top)) return 'unassigned client boundary';
  if (top === 'features' && !featureModules.includes(domain)) return 'unassigned feature module';
  if (top === 'api' && !apiModules.includes((domain ?? '').replace(/\.tsx?$/, ''))) return 'unassigned API module';
  if (top === 'types' && !clientTypeOwners[domain]) return 'unassigned contract owner';
}

// Business ownership is separate from the physical technical layer.
export const clientFeatureOwners = {
  account: 'user', methodology: 'user', portfolio: 'user', evidence: 'user', reviews: 'user', watch: 'user',
  advisor: 'ai', dailyReview: 'ai', settings: 'ai', dashboard: 'support',
};
export const clientApiOwners = {
  account: 'user', methodology: 'user', portfolio: 'user', evidence: 'user', reviews: 'user', watch: 'user',
  analysis: 'ai', dailyReview: 'ai', memory: 'ai', settings: 'ai', activity: 'support',
};
export const clientTypeOwners = { 'user.ts': 'user', 'market.ts': 'market', 'ai.ts': 'ai' };
export const serverModelOwners = {
  accounting: 'user', daily: 'user', fx: 'user', methodology: 'user', options: 'user',
  portfolio: 'user', research: 'user', reviews: 'user', watch: 'user',
  market: 'market',
  analysis: 'ai', connection: 'ai', council: 'ai', daily_review: 'ai',
  endpoint: 'ai', inference: 'ai', memory: 'ai', provider: 'ai',
  decimal: 'support',
};
export const serverApplicationOwners = {
  analysis: 'ai', codex_bridge: 'ai', daily_review: 'ai', memory: 'ai', model_settings: 'ai',
  fx: 'user', portfolio: 'user', prices: 'user', watch: 'user',
  watch_catalog: 'user', dividend_calendar: 'user', watch_export: 'user',
  announcements: 'market', public_prices: 'market', watch_market: 'market', dividend_plans: 'market',
};
export const serverPortOwners = {
  account: 'support', ai_account: 'ai', ai_records: 'ai', analysis: 'ai', model: 'ai', market: 'market', user_account: 'user', user_records: 'user',
};
export const serverHttpOwners = {
  context: 'assembly', error: 'assembly', policy: 'assembly',
  portfolio: 'user', watch: 'user', evidence: 'user', methodology: 'user', reviews: 'user',
  analysis: 'ai', daily_review: 'ai', memory: 'ai', model_settings: 'ai', codex_bridge: 'ai',
};
export const httpCapabilities = {
  'http/portfolio.rs': { context: 'PortfolioContext', ports: ['user_records::LedgerAccount', 'user_account::FxAccount', 'user_account::PortfolioAccount', 'user_account::PortfolioState'] },
  'http/watch.rs': { context: 'WatchContext', ports: ['user_records::WatchManagement', 'user_account::WatchAccount', 'account::AccountSession'] },
  'http/evidence.rs': { context: 'EvidenceContext', ports: ['user_records::EvidenceAccount'] },
  'http/methodology.rs': { context: 'MethodologyContext', ports: ['user_records::MethodologyAccount'] },
  'http/reviews.rs': { context: 'ReviewContext', ports: ['user_records::ReviewAccount'] },
  'http/analysis.rs': { context: 'AnalysisContext', ports: ['ai_records::AnalysisArchive'] },
  'http/daily_review.rs': { context: 'DailyReviewContext', ports: ['ai_account::DailyReviewAccount', 'account::AccountSession'] },
  'http/memory.rs': { context: 'MemoryContext', ports: ['ai_account::MemoryAccount'] },
  'http/model_settings.rs': { context: 'ModelSettingsContext', ports: ['ai_records::AnalysisPresets'] },
  'http/codex_bridge.rs': { context: 'BridgeContext', ports: [] },
};
export function httpCapabilityViolation(source, target) {
  const scope = httpCapabilities[source];
  if (!scope) return;
  if (target === 'http' || target.startsWith('http::')) {
    if (target !== `http::context::${scope.context}`)
      return 'route adapters receive only their declared sealed request context';
  }
  if (target === 'ports' || target.startsWith('ports::')) {
    if (!scope.ports.some(path => target === 'ports::' + path))
      return 'route adapters may import only their declared record capabilities';
  }
}
export function httpRegistryViolations(owners, capabilities, routes) {
  const issues = [];
  for (const [name, owner] of Object.entries(owners)) {
    const source = `http/${name}.rs`;
    if (['user', 'ai'].includes(owner) && (!capabilities[source] || !routes[source]))
      issues.push(`${source}: business routes require an explicit context, capability and route registration`);
  }
  const contexts = Object.values(capabilities).map(scope => scope.context);
  if (new Set(contexts).size !== contexts.length) issues.push('business request contexts must be scoped to one adapter');
  for (const source of Object.keys(capabilities)) {
    if (!['user', 'ai'].includes(owners[source.split('/')[1].replace(/\.rs$/, '')]))
      issues.push(`${source}: stale or non-business request context registration`);
  }
  return issues;
}
export function serverBusinessOwner(source) {
  const [layer, module] = source.replace(/\.rs$/, '').split('/');
  if (layer === 'models') return serverModelOwners[module];
  if (layer === 'application') return serverApplicationOwners[module];
  if (layer === 'ports') return serverPortOwners[module];
  if (layer === 'http') return serverHttpOwners[module];
  if (layer === 'market_data') return 'market';
  return storageOwner(source);
}
export const userAccountCapabilities = {
  'application/fx.rs': ['FxAccount'],
  'application/prices.rs': ['ValuationAccount'],
  'application/portfolio.rs': ['PortfolioAccount', 'PortfolioState'],
  'application/watch.rs': ['WatchAccount'],
};
export function contractOwnerViolation(sourceOwner, targetOwner) {
  if (sourceOwner === 'market' && ['user', 'ai'].includes(targetOwner))
    return 'market contracts cannot depend on user state or AI';
  if (sourceOwner === 'user' && targetOwner === 'ai')
    return 'user contracts cannot depend on AI';
  if (sourceOwner === 'support' && targetOwner !== 'support')
    return 'shared contract primitives cannot depend on business domains';
}
export function serverModelViolation(source, target) {
  if (!target.startsWith('models::') && target !== 'models') return;
  if (target.endsWith('::*')) return 'contract imports must name their module or symbols explicitly';
  const targetOwner = serverModelOwners[target.split('::')[1]];
  if (!targetOwner) return 'import contracts through an explicitly owned module';
  return contractOwnerViolation(serverBusinessOwner(source), targetOwner);
}
export function serverBusinessViolation(source, target) {
  const storeViolation = storageImportViolation(source, target);
  if (storeViolation) return storeViolation;
  const owner = serverBusinessOwner(source);
  if (!owner) return;
  const [layer, module] = target.split('::');
  if (['ai', 'context'].includes(layer)) return contractOwnerViolation(owner, 'ai');
  if (['application', 'ports'].includes(layer)) {
    const targetOwner = (layer === 'application' ? serverApplicationOwners : serverPortOwners)[module];
    if (!targetOwner) return 'cross-module imports must name an explicitly owned application or port';
    if (owner === 'market' && layer === 'ports' && module === 'account')
      return 'public market services do not coordinate account sessions';
    if (owner === 'ai' && targetOwner === 'user')
      return 'AI accesses user data through its authorized read capability, not user workflows or write ports';
    return contractOwnerViolation(owner, targetOwner);
  }
  const ownRepository = ['public_announcements', 'public_prices', 'public_dividends', 'public_dividend_plans']
    .some(name => (source === `db/${name}.rs` || source.startsWith(`db/${name}/`)) && module === name);
  if (owner === 'market' && layer === 'db' && !ownRepository && !['PublicAnnouncements', 'PublicPrices', 'PublicDividends', 'PublicDividendPlans'].includes(module))
    return 'market services may only obtain public market repositories';
  if (source.startsWith('application/') && ['user', 'ai'].includes(owner) && layer === 'db'
      && [undefined, 'self', '*', 'Database'].includes(module))
    return 'business use cases may not obtain the unified account database';
}
export function clientBusinessViolation(source, target) {
  const [layer, name] = source.split('/');
  const [targetLayer, targetName] = target.split('/');
  if (layer === 'types' && targetLayer === 'types')
    return contractOwnerViolation(clientTypeOwners[name], clientTypeOwners[targetName]);
  const sourceOwner = layer === 'features' ? clientFeatureOwners[name]
    : layer === 'api' ? clientApiOwners[name.replace(/\.tsx?$/, '')] : undefined;
  if (sourceOwner === 'user' && targetLayer === 'types' && clientTypeOwners[targetName] === 'ai')
    return 'user modules cannot depend on AI contracts; compose analysis through application callbacks';
  if (layer === 'features' && targetLayer === 'api' && clientFeatureOwners[name] === 'user'
      && clientApiOwners[targetName.replace(/\.tsx?$/, '')] === 'ai')
    return 'user features cannot depend on AI APIs; compose analysis through application callbacks';
}

// Enforce the HTTP seams already split from the former mixed adapters.
export const httpOwnedRoutes = {
  'http/evidence.rs': ['/api/research-evidence'],
  'http/methodology.rs': ['/api/investment-methodology'],
  'http/analysis.rs': ['/api/analysis', '/api/analyses'],
  'http/daily_review.rs': ['/api/daily-review'],
  'http/model_settings.rs': ['/api/model-config', '/api/model-key', '/api/analysis-settings'],
  'http/memory.rs': ['/api/memory'],
  'http/codex_bridge.rs': ['/api/codex-bridge'],
  'http/watch.rs': ['/api/watch'],
  'http/reviews.rs': ['/api/investment-rules', '/api/system-reviews', '/api/review-reminders'],
  'http/portfolio.rs': ['/api/snapshot', '/api/profile', '/api/holdings', '/api/holding-trades', '/api/corporate-actions', '/api/financial-operations', '/api/daily-assets', '/api/portfolio', '/api/market-data', '/api/fx-settings'],
};
export function httpRouteViolation(source, route) {
  const path = route.slice(route.indexOf(' ') + 1);
  for (const [owner, prefixes] of Object.entries(httpOwnedRoutes)) {
    if (prefixes.some(prefix => path === prefix || path.startsWith(prefix + '/')) && source !== owner)
      return `route belongs to ${owner}`;
  }
  if (httpOwnedRoutes[source] && !httpOwnedRoutes[source].some(prefix => path === prefix || path.startsWith(prefix + '/')))
    return 'route is outside this adapter responsibility';
}
