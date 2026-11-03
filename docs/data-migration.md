# 服务器间打包、下载与恢复

频繁搬迁使用一个加密 `.mario-backup` 文件，包含全部账号、业务数据、同步版本及解密用户模型凭据的主密钥。它是整库复制/替换，不合并两台服务器独立产生的修改；多设备实时访问请共用一台中心服务。

部署是宿主机 PostgreSQL，备份无需填写部署模式，也不直接搬运 PostgreSQL 数据目录。

## 第一次搬迁：按顺序执行

现有脚本已经覆盖迁移所需的三个动作，无需手工拼接 SQL：

| 命令 | 执行位置 | 用途 |
| --- | --- | --- |
| `npm run data:backup -- --output 文件` | 旧机器 | 生成一个加密迁移包 |
| `npm run data:download -- --host 用户@旧机器 --remote-dir /srv/mario` | 个人电脑 | 通过 SSH 打包并下载，替代手工打包、下载两步 |
| `npm run data:restore -- --input 文件` | 新机器 | 验证并恢复账号、业务数据和主密钥 |

以下示例采用 `/srv/mario`、默认 `mario.service`，两台服务器通过 root SSH 管理；实际路径、SSH 用户和服务名不同时须替换。Node/数据库操作仍使用 systemd 的非 root 服务用户。所有命令成功后再执行下一步；不要把两端部署与恢复放进忽略错误的批处理。

### 1. 先准备新机器，旧机器继续服务

在旧机器记录实际部署的源码提交号，保存到迁移记录中：

```bash
cd /srv/mario
git rev-parse HEAD
git status --short
systemctl show mario.service --property=User --value
```

如存在影响部署的未提交修改，先保存并确认实际运行版本；不要直接用新机器的最新 main 代替。迁移与升级分开进行，两端优先使用完全相同的 Git 提交。脚本只校验 `package.json` 版本号，不能替你保证同一版本号下的代码相同。

在新机器克隆并检出上面记录的提交，然后提前安装依赖、构建、停服：

```bash
git clone https://github.com/lokinko/mario.git /srv/mario
cd /srv/mario
git checkout <旧机器记录的提交号>
sudo bash deploy.sh --ip <新IP> --domain <正式域名>
sudo bash deploy.sh stop
RUN_USER=$(systemctl show mario.service --property=User --value)
test -n "$RUN_USER" && test "$RUN_USER" != root
sudo install -d -m 700 -o "$RUN_USER" -g "$(id -gn "$RUN_USER")" /srv/mario/backups
```

此时不切 DNS，也不让用户访问新服务。提前按 [部署指南](deploy-madeagents.md#一次性接入已有-nginx) 配好新机器的 HTTPS 证书和 Nginx，安全组只开放所需公网端口。源/目标 PostgreSQL 优先保持相同主版本，目标不能低于源；源 `pg_dump` 不能低于源数据库主版本，目标恢复工具建议与源打包工具同主版本或更高。逻辑备份跨机器迁移的版本约束见 [PostgreSQL 官方文档](https://www.postgresql.org/docs/17/app-pgdump.html#PG-DUMP-NOTES)。

包包含密码哈希（无需知道用户明文密码）、外部登录账号映射、每账号业务表、附件、历史分析、同步版本和模型凭据主密钥。**包不包含整份 `.env`、源码、Nginx/证书、SSH 密钥、个人电脑桥接凭据或 benchmark 文件。** 保留旧机器原配置作为受控运维备份；目标数据库密码、端口等继续使用新机器配置，不要直接覆盖目标 `.env`。

如果启用了 ChatGPT 网站登录，在新机器另行配置原 `MARIO_OPENAI_CLIENT_ID`、认证方式及所需 secret；域名不变时保留原回调，换域名时先完成新回调注册并更新 `MARIO_OPENAI_REDIRECT_URI`。身份映射绑定 client ID，更换 client ID 不能自动继承旧关联；仅有 ChatGPT 登录的账号也不能假定已有可用密码。见 [网站登录配置](chatgpt-sign-in.md)。研究看板需按部署指南重新下载切片、评分，私有仓库认证另行配置。

### 2. 停旧机器，生成最终迁移包

通知用户保存编辑，等正在执行的问答结束。以下在旧机器执行：

```bash
cd /srv/mario
sudo bash deploy.sh stop
RUN_USER=$(systemctl show mario.service --property=User --value)
test -n "$RUN_USER" && test "$RUN_USER" != root
export PATH="/opt/mario-tools/node/bin:$PATH"
sudo -u "$RUN_USER" "$(command -v node)" scripts/backup.mjs --output /srv/mario/backups/mario-final.mario-backup
```

输入并确认至少 12 字符的独立加密密码，单独保管。已有同名文件会拒绝覆盖，再次执行请换文件名。脚本会临时启动已停止的项目数据库完成快照，再关闭它；**旧 API 保持停止，直到迁移验收或明确回退**。外部数据库部署还需停掉连接同一业务库的其他写入实例。

### 3. 把唯一的迁移文件传到新机器

在个人电脑终端执行（PowerShell、Bash 均可）：

```bash
scp root@<旧IP>:/srv/mario/backups/mario-final.mario-backup ./mario-final.mario-backup
scp ./mario-final.mario-backup root@<新IP>:/srv/mario/backups/mario-final.mario-backup
```

非 root SSH 时，需要管理员把包放到该 SSH 用户可读的私有目录，上传到其私有目录后再用 `sudo install` 放入目标 `backups/`，不要放宽整个项目或数据库的权限。仅能从个人电脑访问两台机器时，这种中转方式也可使用。保留 SSH 主机密钥验证；端口、私钥和跳板机优先在个人电脑 `~/.ssh/config` 配置。

### 4. 在新机器恢复，再启动

以下在新机器执行；此时目标服务应仍然停止：

```bash
cd /srv/mario
RUN_USER=$(systemctl show mario.service --property=User --value)
test -n "$RUN_USER" && test "$RUN_USER" != root
sudo chown "$RUN_USER:$(id -gn "$RUN_USER")" backups/mario-final.mario-backup
sudo chmod 600 backups/mario-final.mario-backup
export PATH="/opt/mario-tools/node/bin:$PATH"
sudo -u "$RUN_USER" "$(command -v node)" scripts/restore-backup.mjs --input /srv/mario/backups/mario-final.mario-backup --replace
# 仅在恢复命令成功、且没有提示需修复主密钥配置后启动：
sudo bash deploy.sh start
```

输入第 2 步的密码。第 1 步首次部署会启动 API 并创建 `mario_auth`，即使没有注册账号也不满足“完全没有 Mario schema”的默认恢复条件，因此上面的命令明确使用 `--replace`。它会先生成目标原数据的加密回退包，再替换目标**全部**账号及业务数据，不做账号合并；本流程仅适用于已确认可替换的新目标。如果目标已有需要保留的用户数据，先停止迁移并制定合并方案。仅 `prepare` 且从未启动 API 的空目标可以省略 `--replace`。

恢复成功后，旧会话被撤销，用户需重新登录；原密码保持有效，用户模型 API Key 随主密钥恢复。个人电脑上的 Codex/ChatGPT 桥接需连接目标站点并重新配对，服务器迁移包不提供个人电脑的模型登录凭据。

### 5. 验收新机器后切入口

DNS 尚未切换时，可用本机 hosts 临时把正式域名指向新 IP，浏览器检查新机器；也可先执行以下只读检查（将占位值替换为实际值）：

```bash
curl --fail --resolve <正式域名>:443:<新IP> https://<正式域名>/mario/api/server
```

核对已有账号能登录，关键持仓/现金/成交、研究附件和历史报告存在，模型配置可用；至少用两个已有账号核对隔离。核对恢复后的数据前先不新增或编辑业务记录。新机器不正常时保持旧库和最终包不动，按下节回退。

验收通过后切换 DNS A/AAAA 或上游代理到新机器；如有旧 AAAA 记录也需同步处理。旧机器继续停止，防止 DNS 缓存期间有用户写回旧库。DNS 生效后在新机器运行 `bash deploy.sh check`，再放开用户使用；迁移前执行该检查可能仍命中旧机器，不能据此宣称目标通过。换域名还需通知用户新地址并检查跨域/回调配置。

保留旧数据库、旧配置和最终包，直到完成约定的观察期并有新机器可恢复的备份；之后按保留策略处理包含个人数据的副本。不要将 `backups/` 暴露为静态下载目录。

### 失败回退

- **新机器尚未接收业务写入**：停止目标服务，将 DNS/代理恢复到旧机器，然后在旧机器执行 `sudo bash deploy.sh start`。验收期间如修改过模型设置或账号等数据，也属于新写入，应先核对影响。
- **新机器已经接收新写入**：先停止新机器并打包备份。不要直接启动旧库，否则会丢掉切换后的修改；应将新机器最终包按同样流程恢复到旧机器（目标有数据需 `--replace`），或先人工核对、迁移增量后再切回。
- **恢复失败**：一般数据库事务会回滚；如果明确提示数据库已恢复但 `.env` 替换失败，保持目标停止，按提示修复暂存主密钥配置。不要通过删除数据库、生成新主密钥来重试。锁文件处理见下文。

## 从本机一键打包下载

本机需要 Node 22+ 和 OpenSSH；源服务器有当前项目及 PostgreSQL 工具。使用 SSH 密钥或 agent，保留主机密钥校验：

```bash
npm run data:download -- --host user@server-a --remote-dir /srv/mario
# Windows/Linux 也可以直接执行：
node scripts/download-backup.mjs --host user@server-a --remote-dir /srv/mario
```

运行时输入独立的备份加密密码，不回显。默认下载到本机 `backups/download-时间.mario-backup`，验证密码和认证标签后才公布最终文件；远端使用私有临时目录，不留下长期备份。

该入口使用远端 `node` 命令，不自动提权或执行 `deploy.sh stop`。SSH 用户需具备读取 `.env`、操作项目 PostgreSQL 的权限，且非交互 SSH 的 PATH 中应有 Node。仅由部署器安装专用 Node 时，需为该用户配置 `/opt/mario-tools/node/bin` 的 PATH；root 管理的部署更适合按上面的停服、服务用户打包、SCP 流程执行。日常在线备份可直接下载，最终切换仍须先停旧 API。

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
node scripts/restore-backup.mjs --input ./最新包.mario-backup --replace
sudo bash deploy.sh start
```

上例已首次启动 API，会自动创建账号 schema，因此恢复需 `--replace`；执行前确认目标全部数据允许替换。仅准备配置时用 `bash deploy.sh prepare --ip ... --domain ...`，需已有 Node/PG 工具；从未启动 API 的空库恢复可省略 `--replace`。恢复可初始化空的项目 PostgreSQL 集群，外部数据库需提前创建。已存在 Mario schema 时默认拒绝覆盖，明确替换需加 `--replace`：

```bash
node scripts/restore-backup.mjs --input ./最新包.mario-backup --replace
```

覆盖前自动生成 `backups/before-restore-时间.mario-backup`，使用本次输入的备份密码，包含目标原数据和主密钥。保留以便回退。

恢复先验证密文、SHA-256 和版本，再以一个 PostgreSQL 事务替换所有应用 schemas；失败回滚，清除旧会话。成功只把源 `MARIO_MASTER_KEY` 合并到目标 `.env`，目标域名、数据库密码和其他配置保持不变。源用户用原密码重新登录，模型凭据继续可解密。

数据库与 `.env` 无法跨介质做同一原子事务；如果数据库已提交但配置替换失败，保持服务停止，并按提示将暂存配置替换到 `.env`。强制终止可能留下 `.runtime/restore.lock`，先确认进程停止和数据/主密钥一致再清理；部署脚本会阻止锁存在时启动。

## 密码与空间

包使用 AES-256-GCM、随机盐/nonce 和 scrypt。密码至少 12 字符，不能与备份一起保存，遗失无法解密。备份含所有用户数据，不放公开目录或交给普通用户。

自动化可设置 `MARIO_BACKUP_PASSWORD` 或 `--password-file`，不把密码写进命令参数/`.env`。流式处理避免整库驻留内存；每次 scrypt 约 128 MiB 内存，源/目标/本机都需足够临时磁盘空间。

## 旧版 SQLite 数据

服务端不读取 SQLite。旧单机版的 `mario.db` 需用最后一个包含导出命令的版本转换为同步数据集，再在新账号中导入：

```bash
git worktree add ../mario-legacy legacy-sqlite-export
cd ../mario-legacy
MARIO_DATA_DIR=/path/to/旧资料目录 cargo run --manifest-path server/Cargo.toml -- --export-data ./mario-export.json
```

`MARIO_DATA_DIR` 指向包含 `mario.db` 的目录；导出文件以 `0600` 权限新建，已存在时拒绝覆盖。登录当前版本的新账号后，在“账号与数据”导入该文件；服务端会校验并升级旧数据集版本，失败整次回滚。旧账号、会话与模型密钥不随业务数据导入，需重新配置。导出文件含完整财务资料，导入后删除，并用 `git worktree remove ../mario-legacy` 清理旧版本目录。

## 验证

```bash
MARIO_TEST_PG_BIN=/usr/lib/postgresql/17/bin npm run test:backup
```

测试使用两个独立临时集群，验证加密、凭据迁移、配置保留、会话撤销、服务运行保护、事务回滚、反复替换和回退。禁止将生产数据库传给集成测试。
