import { createDeploymentConfig, defaults, hostname } from './start-common.mjs';
try {
  if (process.argv.length > 3) throw Error('用法：node scripts/generate-env.mjs [mario.example.com]');
  const domain = hostname(process.argv[2] ?? 'localhost');
  if (!createDeploymentConfig(defaults(domain))) console.log('已有 .env，已保留全部配置和密钥；请直接编辑该文件。');
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
