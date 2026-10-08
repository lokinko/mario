import { join, resolve } from 'node:path';
import { createReadStream } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import { args, root } from './start-common.mjs';
import { database, makeBackup, password, timestamp, workspace } from './backup-common.mjs';

let db, work;
try {
  const options = args({ '--help': false, '--mode': true, '--output': true, '--stdout': false, '--password-file': true, '--stdin-password': false });
  if (options['--help']) {
    console.log(`用法：node scripts/backup.mjs [--mode auto|native|docker] [--output 文件.mario-backup]
读取根目录 .env，默认输出 backups/mario-时间.mario-backup。
包含全部账号、业务数据和加密主密钥；使用独立密码加密，交互输入不会回显。
自动化可用 MARIO_BACKUP_PASSWORD 或 --password-file；已有输出不会覆盖。
数据库一致性快照可在线生成；最终搬迁前请停止源 API，避免备份后继续产生数据。`);
  } else {
    if (options['--stdout'] && options['--output']) throw Error('--stdout 与 --output 不能同时使用');
    const passphrase = await password(options, true);
    work = await workspace();
    db = await database(options['--mode']);
    const output = options['--stdout'] ? join(work.path, 'transfer.mario-backup') : resolve(options['--output'] ?? join(root, db.config.MARIO_BACKUP_DIR || 'backups', `mario-${timestamp()}.mario-backup`));
    const path = await makeBackup(db, passphrase, output, work.path);
    if (options['--stdout']) await pipeline(createReadStream(path), process.stdout);
    else console.log(`已生成加密迁移包：${path}\n保存好加密密码；恢复时将撤销旧会话，需要重新登录。`);
  }
} catch (error) {
  console.error(`打包失败：${error.message}`); process.exitCode = 1;
} finally {
  try { await db?.close(); } catch (error) { console.error(error.message); process.exitCode = 1; }
  await work?.dispose();
}
