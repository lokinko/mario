import { existsSync, readFileSync, writeFileSync, createReadStream, createWriteStream, mkdirSync } from 'node:fs';
import { unlink, rename } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { randomBytes } from 'node:crypto';
import { args, root } from './start-common.mjs';
import { database, makeBackup, password, timestamp, unseal, version, workspace } from './backup-common.mjs';

let db, work, lock, stagedConfig, retainStage = false;
const lockPath = join(root, '.runtime/restore.lock');
try {
  const options = args({ '--help': false, '--mode': true, '--input': true, '--replace': false, '--password-file': true, '--stdin-password': false });
  if (options['--help']) {
    console.log(`用法：node scripts/restore-backup.mjs --input 文件.mario-backup [--mode auto|native] [--replace]
先运行 bash deploy.sh prepare 准备配置；脚本可初始化本地空集群，外部数据库需提前创建。
默认仅恢复到完全没有 Mario schema 的空库；--replace 会先加密备份目标再覆盖全部 Mario 数据。
保留目标域名、数据库密码和服务配置，只同步源主密钥；恢复在单事务中完成，旧会话全部撤销。
恢复前请用 sudo bash deploy.sh stop 停止目标 API/Agent。`);
  } else {
    if (!options['--input']) throw Error('请用 --input 指定迁移包');
    const passphrase = await password(options);
    work = await workspace();
    // Authenticate the whole archive before touching configuration, services or database.
    const { dump, metadata } = await unseal(resolve(options['--input']), passphrase, work.path);
    if (metadata.version !== version) throw Error(`源版本 ${metadata.version} 与当前 ${version} 不同，请先使用同版本项目恢复，再按升级说明更新`);
    mkdirSync(join(root, '.runtime'), { recursive: true, mode: 0o700 });
    try { writeFileSync(lockPath, String(process.pid), { flag: 'wx', mode: 0o600 }); lock = true; }
    catch { throw Error('已有恢复操作或遗留 restore.lock；请确认旧进程已停止后再处理锁文件'); }
    const launcherLock = join(root, '.runtime/native/launcher.lock');
    if (existsSync(launcherLock)) {
      const pid = Number(readFileSync(launcherLock, 'utf8'));
      if (!Number.isSafeInteger(pid) || pid < 1) throw Error('原生启动锁无效，请先检查并停止目标服务');
      try { process.kill(pid, 0); throw Error('原生服务仍在运行，请先 Ctrl+C 停止所有目标 API，再恢复'); }
      catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
    db = await database(options['--mode'], { initialize: true });
    if (await db.serverMajor() < metadata.serverMajor) throw Error('目标 PostgreSQL 主版本不能低于源数据库版本');
    const clients = Number(await db.tool('psql', ['-X', '-tA', '-c', "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND backend_type='client backend'"], { capture: true }));
    if (clients !== 0) throw Error('目标数据库仍有其他客户端连接，请停止所有 API 实例与管理连接后重试');
    const currentSchemas = await db.schemas();
    if (currentSchemas.length && !options['--replace']) throw Error('目标已有 Mario 数据；未覆盖。确认需要替换时添加 --replace，脚本会先备份目标');
    if (currentSchemas.length) {
      const safety = resolve(root, db.config.MARIO_BACKUP_DIR || 'backups', `before-restore-${timestamp()}.mario-backup`);
      await makeBackup(db, passphrase, safety, work.path);
      console.log(`目标原数据已加密备份：${safety}（使用本次输入的密码）`);
    }
    // Prepare all files before entering the transaction. Preserve destination-specific settings.
    const envPath = join(root, '.env');
    const original = readFileSync(envPath, 'utf8');
    if (!/^\s*MARIO_MASTER_KEY\s*=/m.test(original)) throw Error('目标 .env 缺少 MARIO_MASTER_KEY');
    const updated = original.replace(/^\s*MARIO_MASTER_KEY\s*=.*$/m, `MARIO_MASTER_KEY=${metadata.masterKey}`);
    stagedConfig = `${envPath}.${randomBytes(8).toString('hex')}.restore`;
    writeFileSync(stagedConfig, updated, { flag: 'wx', mode: 0o600 });
    const sqlPath = join(work.path, 'restore.sql'), transactionPath = join(work.path, 'transaction.sql');
    await db.restoreSql(dump, sqlPath);
    const drop = currentSchemas.map(name => `DROP SCHEMA "${name}" CASCADE;`).join('\n') + '\n';
    async function* transaction() {
      yield Buffer.from(drop); yield* createReadStream(sqlPath);
      yield Buffer.from('\nDELETE FROM mario_auth.sessions;\n');
    }
    await pipeline(Readable.from(transaction()), createWriteStream(transactionPath, { flags: 'wx', mode: 0o600 }));
    if (readFileSync(envPath, 'utf8') !== original) throw Error('恢复期间目标配置发生变化，未写入数据库，请重试');
    await db.tool('psql', ['-X', '--single-transaction', '--set=ON_ERROR_STOP=1', '--file=-'], { input: transactionPath });
    // Keep services stopped if publishing the master key fails after DB commit.
    try { await rename(stagedConfig, envPath); stagedConfig = null; }
    catch { retainStage = true; throw Error(`数据库已恢复，但配置替换失败；请保持 API 停止，将 ${stagedConfig} 替换为 .env 后再启动`); }
    console.log(`已恢复 ${metadata.createdAt} 的迁移包；目标服务配置已保留，主密钥已同步，旧会话已撤销。`);
    if (db.mode === 'native') console.log('请运行原生启动脚本，并重新登录。');
  }
} catch (error) {
  console.error(`恢复失败：${error.message}`); process.exitCode = 1;
} finally {
  try { await db?.close(); } catch (error) { console.error(error.message); process.exitCode = 1; }
  // Retain a staged recovery configuration if DB commit succeeded but rename failed.
  if (stagedConfig && !retainStage) await unlink(stagedConfig).catch(() => {});
  if (lock) await unlink(lockPath);
  await work?.dispose();
}
