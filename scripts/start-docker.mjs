import { resolve } from 'node:path';
import { args, assertNoRestore, createDeploymentConfig, defaults, hostname, readConfig, root, run, validateSecrets } from './start-common.mjs';

try {
  const options = args({ '--help': false, '--domain': true, '--no-build': false });
  if (options['--help']) {
    console.log('用法：node scripts/start-docker.mjs [--domain mario.example.com] [--no-build]\n需要 Node.js 20+、Docker Engine 和 Compose v2。首次创建 .env，之后复用。默认构建并后台启动全部服务。');
  } else {
    const domain = options['--domain'] && hostname(options['--domain']);
    assertNoRestore();
    await run('docker', ['compose', 'version']);
    await run('docker', ['info', '--format', '{{.ServerVersion}}']);
    const path = resolve(root, '.env');
    createDeploymentConfig(defaults(domain));
    const config = readConfig(path);
    if (domain && domain !== config.MARIO_DOMAIN) throw Error('已有 .env 的 MARIO_DOMAIN 与参数不同；请先手动修改该项，密钥保持不变');
    // Explicitly use file values rather than accidentally inheriting another deployment's keys.
    const env = { ...process.env, ...config };
    validateSecrets(env);
    if (!/^[a-fA-F0-9]{32,}$/.test(env.POSTGRES_PASSWORD ?? '')) throw Error('.env 中 POSTGRES_PASSWORD 请使用至少 32 位十六进制值');
    hostname(env.MARIO_DOMAIN);
    const compose = ['compose', '--project-directory', root, '--env-file', path, '-f', resolve(root, 'compose.yaml')];
    await run('docker', [...compose, 'config', '--quiet'], { env });
    assertNoRestore();
    await run('docker', [...compose, 'up', '-d', options['--no-build'] ? '--no-build' : '--build', '--wait', '--wait-timeout', '120'], { env });
    await run('docker', [...compose, 'ps'], { env });
    console.log(`启动完成：https://${env.MARIO_DOMAIN}\n注册邀请码：项目 .env 中的 MARIO_REGISTRATION_KEY\n查看日志：docker compose logs -f --tail=100\n停止服务：docker compose down（保留数据；不要加 -v）`);
  }
} catch (error) {
  console.error(`启动失败：${error.message}`);
  process.exitCode = 1;
}
