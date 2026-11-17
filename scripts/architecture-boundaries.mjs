// Technical registrations; business ownership and data authority: docs/architecture.md.
// New top-level modules must be assigned deliberately before they enter the graph.
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
  shared: ['components', 'lib', 'types.ts'],
};
export const featureModules = ['account', 'dashboard', 'methodology', 'portfolio', 'advisor', 'evidence', 'reviews', 'settings', 'watch'];
export const apiModules = ['account', 'activity', 'memory', 'methodology', 'portfolio', 'analysis', 'evidence', 'reviews', 'settings', 'watch'];
export function boundaryOf(layers, name) {
  return Object.entries(layers).find(([, modules]) => modules.includes(name))?.[0];
}
export function clientBoundaryViolation(path) {
  const [top, domain] = path.split('/');
  if (!boundaryOf(clientLayers, top)) return 'unassigned client boundary';
  if (top === 'features' && !featureModules.includes(domain)) return 'unassigned feature module';
  if (top === 'api' && !apiModules.includes((domain ?? '').replace(/\.tsx?$/, ''))) return 'unassigned API module';
}

// Business ownership is separate from the physical technical layer.
export const clientFeatureOwners = {
  account: 'user', methodology: 'user', portfolio: 'user', evidence: 'user', reviews: 'user', watch: 'user',
  advisor: 'ai', settings: 'ai', dashboard: 'support',
};
export const clientApiOwners = {
  account: 'user', methodology: 'user', portfolio: 'user', evidence: 'user', reviews: 'user', watch: 'user',
  analysis: 'ai', memory: 'ai', settings: 'ai', activity: 'support',
};
export function clientBusinessViolation(source, target) {
  const [layer, name] = source.split('/');
  const [targetLayer, targetName] = target.split('/');
  if (layer === 'features' && targetLayer === 'api' && clientFeatureOwners[name] === 'user'
      && clientApiOwners[targetName.replace(/\.tsx?$/, '')] === 'ai')
    return 'user features cannot depend on AI APIs; compose analysis through application callbacks';
}

// Enforce the HTTP seams already split from the former mixed adapters.
export const httpOwnedRoutes = {
  'http/evidence.rs': ['/api/research-evidence'],
  'http/methodology.rs': ['/api/investment-methodology'],
  'http/analysis.rs': ['/api/analysis', '/api/analyses'],
  'http/model_settings.rs': ['/api/model-config', '/api/model-key', '/api/analysis-settings'],
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
