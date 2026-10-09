# 服务器间打包、下载与恢复

频繁搬迁使用一个加密 `.mario-backup` 文件，包含全部账号、业务数据、同步版本及解密用户模型凭据的主密钥。它是整库复制/替换，不合并两台服务器独立产生的修改；多设备实时访问请共用一台中心服务。

当前部署是宿主机 PostgreSQL，备份默认 `native`，无需填写部署模式。旧 Docker 实例可在旧版本导出同格式加密包，再在本版本宿主机恢复；不会直接搬运 PostgreSQL 数据目录。

## 从本机一键打包下载

本机需要 Node 22+ 和 OpenSSH；源服务器有当前项目及 PostgreSQL 工具。使用 SSH 密钥或 agent，保留主机密钥校验：

```bash
npm run data:download -- --host user@server-a --remote-dir /srv/mario
# Windows/Linux 也可以直接执行：
node scripts/download-backup.mjs --host user@server-a --remote-dir /srv/mario
```

运行时输入独立的备份加密密码，不回显。默认下载到本机 `backups/download-时间.mario-backup`，验证密码和认证标签后才公布最终文件；远端使用私有临时目录，不留下长期备份。

频繁使用可将 `MARIO_BACKUP_SSH_HOST`、`MARIO_BACKUP_REMOTE_DIR`、SSH 端口/私钥路径写入本机 `.env`，之后只需 `npm run data:download`。数据库连接、PG 工具路径及主密钥来自源服务器自己的 `.env`。

## 源服务器只打包

```bash
cd /srv/mario
npm run data:backup
# 或 bash scripts/backup.sh
```

备份使用 `pg_dump` 一致性快照，只导出 `mario_auth` 和 `user_<uuid>` schemas。源码、证书、其他应用数据与 PostgreSQL 角色不包含在包里。数据库停止时临时启动，结束后关闭；已运行数据库保持运行。

日常可在线备份。最终搬迁前等待 AI 操作结束并停服，生成最后一份备份，切换期间保持源服务停止，避免数据分叉：

```bash
sudo bash deploy.sh stop
npm run data:backup
```

备份、恢复操作应以服务运行用户执行；root 登录部署时默认为 `mario`，可通过 `sudo -u mario` 执行 Node 工具，专用 Node 为 `/opt/mario-tools/node/bin/node`。已有数据库 URL 使用已设置的连接，账户需能读取全部应用 schemas。

## 目标恢复

先在目标安装同版本项目和工具，用 `deploy.sh prepare` 生成配置，保持服务停止。新目标可先完成首次部署再 stop；这样依赖和二进制已准备完毕。PG 主版本不能低于源库。

```bash
cd /srv/mario
sudo bash deploy.sh --ip 目标IP --domain 目标域名
sudo bash deploy.sh stop
# 上传迁移包后，作为服务运行用户执行：
node scripts/restore-backup.mjs --input ./最新包.mario-backup
sudo bash deploy.sh start
```

仅准备配置时用 `bash deploy.sh prepare --ip ... --domain ...`，需已有 Node/PG 工具。恢复可初始化空的项目 PostgreSQL 集群，外部数据库需提前创建。已存在 Mario 数据时默认拒绝覆盖，明确替换需加 `--replace`：

```bash
node scripts/restore-backup.mjs --input ./最新包.mario-backup --replace
```

覆盖前自动生成 `backups/before-restore-时间.mario-backup`，使用本次输入的备份密码，包含目标原数据和主密钥。保留以便回退。

恢复先验证密文、SHA-256 和版本，再以一个 PostgreSQL 事务替换所有应用 schemas；失败回滚，清除旧会话。成功只把源 `MARIO_MASTER_KEY` 合并到目标 `.env`，目标域名、数据库密码和其他配置保持不变。源用户用原密码重新登录，模型凭据继续可解密。

数据库与 `.env` 无法跨介质做同一原子事务；如果数据库已提交但配置替换失败，保持服务停止，并按提示将暂存配置替换到 `.env`。强制终止可能留下 `.runtime/restore.lock`，先确认进程停止和数据/主密钥一致再清理；部署脚本会阻止锁存在时启动。

## 密码与空间

包使用 AES-256-GCM、随机盐/nonce 和 scrypt。密码至少 12 字符，不能与备份一起保存，遗失无法解密。备份含所有用户数据，不放公开目录或交给普通用户。

自动化可设置 `MARIO_BACKUP_PASSWORD` 或 `--password-file`，不把密码写进命令参数/`.env`。流式处理避免整库驻留内存；每次 scrypt 约 128 MiB 内存，源/目标/本机都需足够临时磁盘空间。

## 验证

```bash
MARIO_TEST_PG_BIN=/usr/lib/postgresql/17/bin npm run test:backup
```

测试使用两个独立临时集群，验证加密、凭据迁移、配置保留、会话撤销、服务运行保护、事务回滚、反复替换和回退。禁止将生产数据库传给集成测试。
