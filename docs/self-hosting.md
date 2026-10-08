# 自建服务器部署

当前架构支持多个独立用户：React 网页与 Tauri 客户端共用 Rust API；PostgreSQL 保存账号、会话和各用户业务数据；AI Agent 是独立的无状态服务。已移除旧托管账户、邮件回跳和云端快照同步实现。

## 只维护一个配置文件

Docker 和无 Docker 启动脚本统一读取**项目根目录 `.env`**。[`.env.example`](../.env.example) 列出全部可配置项、默认值、用途及适用方式。生成的真实配置同样带有完整中文注释：

```bash
node scripts/generate-env.mjs
# 编辑 .env，通常只需设置 MARIO_DOMAIN。
bash scripts/start-docker.sh
# 或选择无 Docker 方式：bash scripts/start-native.sh
```

生成器会自动填写数据库密码、加密主密钥、邀请码和 Agent 令牌，已有 `.env` 不会被覆盖。不要直接复制带空密钥的模板后启动。原生部署还可在文件中设置 `PG_BIN`、`DATABASE_URL`、端口，以及 `MARIO_NATIVE_HTTPS=true`，无需在命令中反复传参。`DATABASE_URL` 和 `PG_BIN` 的进程环境变量仍支持临时覆盖，其余启动配置以文件为准。

旧版只有 `.env.native` 时会自动迁移为 `.env`，原文件保留为 `.env.native.backup`，原有密钥保持不变；两份配置同时存在时会停止并提示手动合并，避免使用错误密钥。**统一配置不代表统一数据目录**：Docker 数据卷与原生 PostgreSQL 数据目录仍各自独立，切换方式需要导出/恢复数据。用户各自的模型名称与 API Key 在登录后的“模型与隐私”中设置。

## Docker Compose

服务器安装 Docker Engine 与 Compose，域名 A/AAAA 记录指向服务器，开放 80/443。

```bash
git clone <本项目仓库地址>
cd mario
bash scripts/start-docker.sh --domain mario.example.com
docker compose logs --tail=100 api agent
```

启动脚本需要 Node.js 20+、Docker Engine 与支持 `up --wait` 的 Compose v2。脚本检查 Docker、首次生成 `.env`、构建并在后台启动四个服务，后续启动复用原配置，不会覆盖密钥。如果已有 `.env`，`--domain` 必须与其中的 `MARIO_DOMAIN` 一致；更改域名只需编辑该项。也可复制 `.env.example` 手动配置。数据库密码使用至少 32 位十六进制，避免连接 URL 转义问题。Caddy 自动申请与续期 HTTPS 证书；使用 `localhost` 时是本地 CA，需要将其证书导入信任库。

```bash
bash scripts/start-docker.sh             # 再次启动或拉取新代码后重新构建
bash scripts/start-docker.sh --no-build  # 使用已构建的镜像
docker compose ps
docker compose down                     # 停止，保留数据；不要加 -v
```

也可以使用 `npm run start:docker -- --domain mario.example.com`。Docker 服务需配置开机启动，容器使用 `restart: unless-stopped` 自动恢复。

访问 `https://mario.example.com`，使用管理员从 `.env` 获取的 `MARIO_REGISTRATION_KEY` 邀请码注册。每人使用自己的邮箱、密码和模型密钥。邮箱作为账户名，不做邮件验证或邮件重置；邀请码应仅分发给可信用户，轮换它不影响已有账号。

| 服务 | 作用 | 公网访问 |
| --- | --- | --- |
| web | 静态网页、HTTPS、反向代理 | 80/443 |
| api | 账号认证、领域规则、授权上下文、业务写入 | 无，仅 web 转发 |
| database | PostgreSQL 17，持久卷 `postgres-data` | 无，仅 API 所在内部网络 |
| agent | 模型调用、工作流、结构化输出验证 | 无，仅内部令牌授权 |

Agent 不获得 `DATABASE_URL`、数据库网络或 `MARIO_MASTER_KEY`。它收到本次授权上下文及本次调用所需的模型密钥，完成后由 API 保存结果。原生模型 API Key 从不发送到浏览器；用户在设置页输入时通过 HTTPS 传给 API。行情查询由 API 的市场数据适配器执行。

## 无 Docker 一键启动

使用普通系统用户，预先安装 Node.js 20+、Rust 1.86+ 与 C/C++ 编译工具链，以及 PostgreSQL 17 的 `initdb`、`pg_ctl`。脚本不自动安装系统软件、不修改已有系统数据库服务。PostgreSQL 工具不在 PATH 时，用 `PG_BIN` 指定它的 `bin` 目录（例如 `/usr/lib/postgresql/17/bin`）。

```bash
cd mario
PG_BIN=/usr/lib/postgresql/17/bin bash scripts/start-native.sh
# 工具已在 PATH 时：bash scripts/start-native.sh
```

首次运行自动安装前端依赖、编译网页和 Rust 服务，创建独立 `.env` 密钥文件，在 `.runtime/native/postgres` 初始化本地 PostgreSQL，然后启动独立 Agent 和 API。API 同时提供网页，地址为 `http://127.0.0.1:4217`。默认 PostgreSQL 端口 55432、Agent 端口 4218；它们都只监听回环地址。邀请码在 `.env` 的 `MARIO_REGISTRATION_KEY`。配置、数据库和运行日志不进入 Git 或容器构建上下文。

```bash
# 后续启动可跳过构建；更新代码后应去掉 --skip-build
bash scripts/start-native.sh --skip-build

# 已有数据库：首次启动前设置连接地址（URL 中的特殊字符需编码）
export DATABASE_URL='postgresql://mario:your_password@127.0.0.1:5432/mario'
bash scripts/start-native.sh
```

已有数据库地址会在首次生成 `.env` 时保存，也可手动修改文件中的 `DATABASE_URL`；当前进程的 `DATABASE_URL` 优先。配置了它后，脚本不会初始化、启动或停止该数据库。数据库需要提前创建，并授予应用账户创建 schema 的权限。内置本地数据库使用初始 `postgres` 数据库保存应用 schema，不会操作系统安装的其他集群。端口可在 `.env` 的 `MARIO_PORT`、`MARIO_AGENT_PORT`、`MARIO_PG_PORT` 中修改。不要删除 `.env` 后复用旧数据目录；应从备份恢复原密钥和密码。

原生脚本**前台运行**，按 Ctrl+C 会停止本次启动的 API、Agent、Caddy 和本地数据库，保留所有数据。关闭 SSH 会话后不保证继续运行；需要持续运行时可在 tmux 会话中启动，或自行配置 systemd 托管。脚本检测重复启动和端口冲突，不会杀掉其他服务。`.runtime/native/postgres.log` 保存本地数据库日志，其余日志输出到终端。

### 原生部署启用 HTTPS

安装 Caddy，将域名解析到服务器并开放 80/443。运行用户必须具备绑定这两个端口的权限；Linux 可由管理员为已安装的 Caddy 可执行文件授予 `cap_net_bind_service`，不要为运行 PostgreSQL 而将整个脚本切换为 root。已有 Caddy/Nginx 占用端口时，复用它转发到 `127.0.0.1:4217`，不要加 `--domain`。

```bash
# 首次生成配置时指定域名；已有 .env 时先修改其中的 MARIO_DOMAIN
bash scripts/start-native.sh --domain mario.example.com
```

该参数使用 `deploy/Caddyfile.native` 同时启动 Caddy，证书保存在 `.runtime/native/caddy-data`；访问 `https://mario.example.com`。也可以只在 `.env` 中设置 `MARIO_DOMAIN=mario.example.com` 和 `MARIO_NATIVE_HTTPS=true`，后续直接运行脚本即可；首次通过 `--domain` 创建配置时会自动保存这一开关。只运行默认本机 HTTP 时，可以通过 SSH 隧道访问：`ssh -L 4217:127.0.0.1:4217 user@server`，再在本机打开 `http://127.0.0.1:4217`。

Windows 可用 `node scripts/start-native.mjs --skip-build` 和 `node scripts/start-docker.mjs --domain mario.example.com`，或对应的 `npm run start:native` / `npm run start:docker`；不需要 Bash。自动构建仍需 Rust/MSVC 工具链，`PG_BIN` 指向 PostgreSQL 的 `bin` 目录。

无 Docker 备份时，使用 `pg_dump` 对 `.env` 中的外部数据库，或本地 `127.0.0.1:55432` 的 `postgres` 数据库（用户 `mario`）做备份；同时单独备份 `.env`。下文的 `docker compose exec` 管理命令仅适用于 Docker 部署。

## 配置

日常部署请以带中文注释的 `.env` / `.env.example` 为准。下表中的 `MARIO_HOST`、`MARIO_WEB_DIR`、`MARIO_ROLE`、Agent 地址是直接运行二进制时的高级参数，一键脚本自动设置，无需再维护一份配置。Docker 忽略模板中标为“仅用于无 Docker”的参数。

| 环境变量 | 含义 |
| --- | --- |
| `DATABASE_URL` | PostgreSQL 连接。设置后进入多用户模式，不接受原本的共享访问令牌 |
| `MARIO_MASTER_KEY` | Base64 编码的 32 字节随机主密钥，用于 ChaCha20-Poly1305 加密每个账号的供应商密钥 |
| `MARIO_REGISTRATION_KEY` | 至少 32 字符邀请码；不配置时注册关闭 |
| `MARIO_AGENT_URL` / `MARIO_AGENT_TOKEN` | 内部 Agent 地址与独立共享令牌，令牌至少 32 字符 |
| `MARIO_MODEL_HOSTS` | 允许访问的 HTTPS 模型主机，逗号分隔；默认 OpenAI、Anthropic 官方主机，仅允许 443 |
| `MARIO_ALLOWED_ORIGINS` | 额外网页来源，逗号分隔的完整 Origin；同源网页无需配置，Tauri 来源已内置 |
| `MARIO_HOST` | API 监听地址，容器为 `0.0.0.0`，独立运行默认 `127.0.0.1` |
| `MARIO_WEB_DIR` | 可选静态网页目录；Compose 使用独立 web 容器，无需设置 |
| `MARIO_ROLE=agent` | 独立 Agent 进程模式，监听 `MARIO_AGENT_BIND`，默认 `127.0.0.1:4218` |

当前 PostgreSQL 驱动通过容器内部网络连接，不启用数据库连接 TLS；跨主机数据库请放在受保护的专用网络/隧道，勿直接暴露 5432。模型自定义域名属于管理员信任边界，添加前确认解析目标。公网大规模部署还应在入口配置 IP 请求限速；内置登录每账户每分钟 10 次、每 API 进程最多 2 个同时进行的密码运算。

## 网页与客户端

网页使用同源 `/api`。桌面/Android 客户端在登录页填写 `https://mario.example.com`，不再启动 sidecar、不再内嵌数据库或 API 服务。会话只保存在当前窗口的 `sessionStorage`，关闭窗口后需要重新登录；服务端会话有效期 7 天，退出会立即撤销该会话。

```bash
npm ci --prefix client
npm run windows:build     # Windows，需 Rust/MSVC 与 WebView2
npm run desktop:build    # macOS，在 macOS 上构建
```

开发前端可设置 `VITE_API_URL=http://127.0.0.1:4217/api` 连接本地运行的多用户 API。生产远程地址必须 HTTPS。没有 `VITE_API_URL` 的 Vite 开发模式保留 SQLite 本地业务调试路径，不代表生产部署模式。

## 数据模型与同步

账号与会话在 `mario_auth` schema，用户数据在服务器由 UUID 生成的 `user_<uuid>` schema 中，财务档案、持仓、流水、决策等仍为独立关系表。租户由已验证的会话确定，客户端不能选择 schema 或传入 user ID 越权。数据库账户仅供后端使用，终端用户没有 PostgreSQL 登录权限。

每个用户有数据库持久化版本号，业务行更改的触发器与修改在同一事务提交。客户端从快照响应的 `x-data-revision` 取得版本，写入携带 `If-Match`；缺失或过期返回 409。API 使用 PostgreSQL 的账户级 advisory lock 完成版本检查和写入，多个 API 实例仍遵守同一顺序。不同用户互不等待。

客户端合并检查请求、只在前台检查、空闲时逐步降低频率（60 秒至 5 分钟）、断网后指数退避。`GET /api/sync/version` 仅返回版本，变化时才读取最新快照。有未提交输入时提示核对并阻止旧表单提交。删除也推进版本，因此所有设备都能看到删除。

**这是在线共享数据库模式，离线不能保存，也没有后台离线写入队列。** 网络超时后写入可能已提交，应刷新确认，客户端不会盲目重试写入。持仓金额接口和 CSV 流水保留已有业务幂等机制。版本粒度是整个账户，独立记录的并发修改也可能要求刷新；这是保护旧表单的保守策略。

AI 调用前冻结授权上下文并校验预览指纹，随后释放账户锁，其他设备可以继续编辑。结果记录分析时的上下文，不改写当前资产。Agent 超时或失败不生成成功记录。当前每个 API 进程最多 24 个业务请求；数据库连接按请求打开并关闭，适合个人团队/小规模自建，尚未实现连接池、大规模租户管理和持久 AI 作业队列。

## 迁移旧数据

先关闭旧客户端并备份其 SQLite 数据目录。若最新数据只在旧云端，应先用旧版本恢复到本地；新版没有旧云服务连接器。

```bash
# MARIO_DATA_DIR 指向原数据库 mario.db 所在目录。
MARIO_DATA_DIR=/path/to/old-data cargo run --manifest-path server/Cargo.toml -- --export-data /safe/mario-export.json
```

Windows PowerShell：

```powershell
$env:MARIO_DATA_DIR='D:\backup\mario-data'
cargo run --manifest-path server/Cargo.toml -- --export-data D:\backup\mario-export.json
```

导出不会覆盖已有文件。登录新账号，在“账号与数据”导入 JSON。只允许向空账号导入，不覆盖已存在的业务记录；空账号自动生成的每日设置/空缓存不阻止导入。JSON 的 schema、外键关联、金额/汇率来源、流水标识和冲正关系仍执行现有校验，事务失败回滚。默认请求上限 8 MiB。供应商密钥、旧登录信息、通知设置不导入，请重新配置。

导出文件是**明文个人资料**。服务器能处理业务明文，当前模式不再声称端到端加密；TLS 保护传输，主密钥仅加密供应商凭据。数据库磁盘加密由服务器操作系统/存储提供。

## 备份、恢复、密码与升级

频繁服务器搬迁可用 `bash scripts/backup.sh` 打包、`bash scripts/download-backup.sh` 从本机通过 SSH 一键打包下载，以及 `bash scripts/restore-backup.sh --input 文件.mario-backup` 恢复。包包含全部账号数据与主密钥并独立加密，支持 Docker/原生互相迁移。完整命令、覆盖前自动备份与回退步骤见 [服务器间迁移指南](data-migration.md)。下方 `pg_dump` 命令适用于管理员手动维护。

数据库使用 PostgreSQL 标准备份工具，建议每日备份到服务器之外，并定期在隔离数据库演练恢复。

```bash
# Linux/macOS shell：pg_dump 输出在容器内创建，再复制出来。
docker compose exec database pg_dump -U mario -d mario -Fc -f /tmp/mario.dump
docker compose cp database:/tmp/mario.dump ./mario.dump
# 恢复到新的空数据库或新 Compose 环境，停止 API 避免并发写入。
docker compose stop api
docker compose cp ./mario.dump database:/tmp/restore.dump
docker compose exec database pg_restore -U mario -d mario --clean --if-exists /tmp/restore.dump
docker compose start api
```

备份 `.env` 中的 `MARIO_MASTER_KEY` 并单独保管；丢失后不能解密供应商密钥。不要用生成脚本覆盖主密钥。仅更换主密钥会使已有密钥失效，当前没有自动轮换命令，轮换需要用户重新保存供应商密钥。`docker compose down` 保留数据卷；不要执行 `down -v`，除非明确要删除数据库。

忘记密码时由管理员重置，密码通过环境传递而不是命令行参数，操作会撤销该账号全部会话：

```bash
read -rs MARIO_RESET_PASSWORD; export MARIO_RESET_PASSWORD
docker compose exec -e MARIO_RESET_PASSWORD api mario-server --reset-password person@example.com
unset MARIO_RESET_PASSWORD
```

生产更新前先备份：拉取代码，`docker compose up -d --build`。首次访问账号会创建 PostgreSQL 关系表；SQLite 旧版本迁移保持独立。跨未来 schema 版本升级需跟随版本迁移说明，不要在旧 API 和新 API 间长期混用。

## 验证

```bash
npm run build --prefix client
npm test --prefix client
cargo test --manifest-path server/Cargo.toml
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo build --manifest-path server/Cargo.toml
MARIO_TEST_DATABASE_URL=postgresql://test:password@127.0.0.1:5432/mario_test node scripts/test-hosted.mjs
```

集成测试创建随机账号与业务数据，必须使用临时数据库。它同时启动两个 API 实例，验证多账号隔离、并发写冲突、密钥与会话、导入、重启持久性，以及完整领域流程。CI 的 `hosted.yml` 使用临时 PostgreSQL 并构建两个容器镜像。

启动脚本另有真实 PostgreSQL 冒烟测试：先构建 `client/dist` 和后端 debug 二进制，再执行 `MARIO_TEST_PG_BIN=/usr/lib/postgresql/17/bin npm run test:startup`。测试在临时目录创建独立集群，检查首次启动、注册、重复启动保护、重启保留数据/密钥、故障清理及外部数据库生命周期，完成后清理测试数据。
