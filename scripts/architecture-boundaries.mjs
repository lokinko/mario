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
export const featureModules = ['account', 'dashboard', 'methodology', 'portfolio', 'research', 'reviews', 'settings', 'watch'];
export const apiModules = ['account', 'activity', 'memory', 'portfolio', 'research', 'reviews', 'settings', 'watch'];
export function boundaryOf(layers, name) {
  return Object.entries(layers).find(([, modules]) => modules.includes(name))?.[0];
}
export function clientBoundaryViolation(path) {
  const [top, domain] = path.split('/');
  if (!boundaryOf(clientLayers, top)) return 'unassigned client boundary';
  if (top === 'features' && !featureModules.includes(domain)) return 'unassigned feature module';
  if (top === 'api' && !apiModules.includes((domain ?? '').replace(/\.tsx?$/, ''))) return 'unassigned API module';
}
