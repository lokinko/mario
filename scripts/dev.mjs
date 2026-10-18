// Local development: persistent PostgreSQL under .runtime/dev, the Rust API on :4217 and Vite on :1420.
// Accounts are registered in the browser like production; data stays in .runtime/dev until deleted.
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { developmentCluster, root } from './postgres-cluster.mjs';

const port = Number(process.env.MARIO_DEV_PG_PORT || 54329);
const cluster = await developmentCluster(port);
const children = [];
let stopping = false;
async function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  await Promise.all(children.filter(child => child.exitCode === null).map(child => { child.kill('SIGTERM'); return once(child, 'exit'); }));
  await cluster.stop().catch(error => console.error(error.message));
  process.exit(code);
}
function launch(name, command, args, env = {}) {
  const child = spawn(command, args, { cwd: root, stdio: 'inherit', env: { ...process.env, ...env } });
  child.once('exit', code => { if (!stopping) { console.error(`${name} 已退出 (${code})`); void stop(code ?? 1); } });
  children.push(child);
}
const serverEnv = { DATABASE_URL: await cluster.createDatabase('mario_dev'), MARIO_MASTER_KEY: cluster.masterKey };
delete process.env.MARIO_AGENT_URL;
launch('server', 'cargo', ['run', '--manifest-path', 'server/Cargo.toml'], serverEnv);
launch('client', 'npm', ['run', 'dev', '--prefix', 'client']);
for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.once(signal, () => void stop());
