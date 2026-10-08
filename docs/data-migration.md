# 服务器间打包、下载与恢复

频繁搬迁推荐使用 **加密 PostgreSQL 快照包**：一个 `.mario-backup` 文件包含全部账号、密码哈希、业务数据、版本号和解密用户模型密钥所需的主密钥。无需搬运 PostgreSQL 数据目录，Docker 与原生部署之间也能迁移。它是整库复制/替换，不会合并两个服务器各自的修改；需要双向实时同步时应使用同一个中心数据库。

## 在本机一键打包并下载

本机需要 Node.js 20+ 和 OpenSSH 客户端；Linux/macOS 源服务器需有更新后的项目和可运行的数据库工具。建议使用 SSH 密钥或 SSH agent，主机密钥仍按系统 SSH 规则验证。

```bash
# 在本机的项目根目录执行；在远端打包，直接下载到本机。
bash scripts/download-backup.sh --host user@server-a --remote-dir /srv/mario --mode docker
# 无 Docker 的源服务器改为 --mode native。
# Windows 不需要 Bash：
node scripts/download-backup.mjs --host user@server-a --remote-dir /srv/mario --mode docker
```

运行时输入独立的备份加密密码，不回显；SSH 密钥保护传输，密码保护落盘的包。默认文件位于本机 `backups/download-时间.mario-backup`。下载会验证完整包的密码与认证标签，通过后才公布最终文件；同名文件不覆盖，失败清理临时文件。远端只使用私有临时目录，不留下长期备份包。

频繁下载可将以下字段写入**本机** `.env`，之后只需 `bash scripts/download-backup.sh` 或 `npm run data:download`：

```dotenv
MARIO_BACKUP_MODE=docker
MARIO_BACKUP_DIR=backups
MARIO_BACKUP_SSH_HOST=user@server-a
MARIO_BACKUP_REMOTE_DIR=/srv/mario
MARIO_BACKUP_SSH_PORT=22
MARIO_BACKUP_SSH_IDENTITY=
```

源服务器自己的 `.env` 提供数据库连接、主密钥和 PostgreSQL 工具路径。`MARIO_BACKUP_MODE=auto` 在存在原生数据目录/外部数据库 URL 时选择原生，否则选择 Docker；同一目录使用过两种方式时应显式选择，避免备份错数据库。命令行的 `--host`、`--remote-dir`、`--mode`、`--port`、`--identity`、`--output` 优先于文件设置。

## 只在源服务器打包

```bash
cd /srv/mario
bash scripts/backup.sh --mode docker
# 原生：bash scripts/backup.sh --mode native
# 自定义输出路径：
node scripts/backup.mjs --mode native --output /safe/mario-current.mario-backup
```

默认使用 `.env` 中的 `MARIO_BACKUP_DIR`。脚本调用 `pg_dump` 的一致性快照，只备份 `mario_auth` 和应用的 `user_<uuid>` schemas；数据库角色、其他应用数据、程序代码、HTTPS 证书不在包中。源模型密钥保持原有加密形式，主密钥位于包的加密载荷中。现有本地 PostgreSQL 停止时，脚本可临时启动它并在结束后停止；已经运行的数据库不会被停止。外部数据库需要运行且允许备份账户读取全部应用 schemas。

可以在线生成定期备份。**最终迁移前请停止源服务器的所有 API 实例，等待 AI 操作结束，再生成最后一个包**；备份后产生的写入不会自动进入目标。Docker 可执行 `docker compose stop api agent`；原生一键启动可 Ctrl+C，备份脚本会临时启动数据库。切换完成前保留源服务停止状态，避免两端同时写入产生分叉。

## 在目标服务器恢复

目标需安装相同版本的项目（当前 0.5.0），PostgreSQL 主版本不得低于源数据库。先生成/填写目标 `.env`，保留目标自己的域名、数据库密码、Agent 令牌和邀请码。将迁移包通过 `scp` 等方式上传：

```bash
# 本机上传，不会删除本机原包。
scp ./backups/download-时间.mario-backup user@server-b:/srv/mario/

# 目标服务器：先准备配置，再恢复，最后启动。
cd /srv/mario
node scripts/generate-env.mjs server-b.example.com
bash scripts/restore-backup.sh --mode docker --input ./download-时间.mario-backup
bash scripts/start-docker.sh
```

原生目标使用 `--mode native`，在 `.env` 中填写 `PG_BIN`（不在 PATH 时），之后执行 `bash scripts/start-native.sh`。首次恢复会自动初始化本地空 PostgreSQL 集群，或启动 Docker 的 database 容器；外部数据库需提前创建。只有 database 容器的阶段不提供网站，恢复完成后再启动全部服务。

目标已有 Mario schema 时默认拒绝，反复搬运明确加 `--replace`：

```bash
bash scripts/restore-backup.sh --mode docker --input ./最新包.mario-backup --replace
# 原生部署需先停止所有 API，然后运行 --mode native 的同一命令。
```

覆盖前自动生成 `backups/before-restore-时间.mario-backup`，使用本次输入的备份密码，包含目标原数据与原主密钥。请保留直到验证完成；需要回退时把它作为 `--input` 再恢复。目标若存在不完整的账号 schema 导致无法备份，脚本拒绝继续，需管理员先修复/单独备份。

恢复先验证全部密文、SHA-256 与项目版本，再停止本项目 Docker API/Agent（完成或失败后恢复原先运行的服务）。原生脚本检查启动锁和数据库连接，要求所有 API/管理连接已停止。数据替换在一个 PostgreSQL 事务里完成，SQL 失败回滚；清除所有旧用户 schemas，防止残留目标独有账户的数据。成功后只将源 `MARIO_MASTER_KEY` 合并到目标 `.env`，其余目标配置保持原值。旧会话全部撤销，用户使用原账号密码重新登录，模型密钥可继续解密。

配置文件与数据库无法跨介质做同一原子事务；如果数据库已提交但 `.env` 替换失败，脚本会保持 API 停止并给出暂存配置路径，按提示将它替换为 `.env` 后再启动。强制终止恢复进程可能留下 `.runtime/restore.lock`；先确认进程停止、核对数据/主密钥，再清理锁。启动脚本检测该锁，避免在恢复过程中启动业务服务。

## 密码、权限与容量

迁移包使用 AES-256-GCM，随机盐和 nonce，scrypt 派生加密密钥。密码至少 12 字符，不能与包一起保存，遗失无法恢复；备份包含所有用户数据，不应交给普通账号用户或放入网页公开目录。

自动化时可用 `MARIO_BACKUP_PASSWORD` 环境变量，或 `--password-file /private/backup-password`；密码文件应设为只有本人可读。不要用命令参数传入密码，也不要将密码写入 `.env`。数据库密码只通过子进程环境传递，不进入命令参数。SSH 下载时备份密码通过 SSH stdin 传递。源/目标/本机都需要足够磁盘空间；恢复会短暂保存权限受限的解密数据和 SQL，正常完成后清理。

脚本流式处理大文件，不将整库读入内存；scrypt 每次约使用 128 MiB 内存。自定义备份目录需位于非公开路径。Docker 的备份工具来自 PostgreSQL 容器；原生 `pg_dump`、`pg_restore`、`psql` 应使用 PostgreSQL 17 或与源/目标版本兼容的同组工具。备份日志不输出密码或主密钥。

## 验证

```bash
# 已构建后端 debug 二进制，PostgreSQL 工具路径按服务器调整。
MARIO_TEST_PG_BIN=/usr/lib/postgresql/17/bin npm run test:backup
```

测试使用两个独立临时集群，覆盖账号/财务数据/凭据迁移、密码错误、密文损坏、空目标恢复、目标配置保留、会话撤销、在线服务保护、SQL 回滚、反复替换、孤立租户清理和自动备份回退。脚本不会操作测试目录以外的数据库。
