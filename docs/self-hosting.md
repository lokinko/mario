# 自建宿主机服务

生产部署统一使用根目录 [deploy.sh](../deploy.sh)，所有日常配置集中在脚本顶部并有中文说明。以下入口为 `https://lokinko.cn/mario/`，源码直接在宿主机编译和运行，systemd 负责后台常驻和开机自启。

```bash
sudo bash deploy.sh --ip 47.106.188.95 --domain lokinko.cn
```

只有 `deploy` 显式同时传入 `--domain`、`--ip` 时才调用独立域名脚本，配置 Nginx、主域名与 www 跳转，并复用双域名证书或通过 HTTP 验证申请。日常 `sudo bash deploy.sh` 不调用它。DNS、证书失败回退及已有网站手工接入见 [服务器部署指南](deploy-madeagents.md)。`.env` 由部署器自动生成，保存运行参数和持久密钥；日常改 `deploy.sh` 后重新部署，密码和主密钥保持不变。真实 `.env` 不提交 Git。

Web 通过 Rust API 读写数据，PostgreSQL 保存独立账号、会话及各用户关系数据，AI Agent 是独立无状态进程。三个服务仅监听回环地址，公网访问由已有 Nginx 提供 HTTPS。Agent 不获得数据库 URL 或主密钥，它仅收到本次授权上下文和模型请求所需凭据。

## 账号与客户端

注册开放，任何用户均可用邮箱和密码直接创建账号，无需邀请码。每个人有独立邮箱、密码与模型密钥；邮箱作为账号名，不做邮件验证/邮件重置。通过浏览器访问配置的域名及 `/mario/` 路径。

登录凭证及过期时间按服务器地址保存在浏览器的 `localStorage`，刷新、关闭后重开仍可恢复登录；服务端会话有效期 7 天，到期需重新登录。退出会撤销当前会话并清除本机凭证，同一浏览器的其他标签页同步更新登录状态。数据库临时错误不会被当作会话失效。网页、API 和资源统一使用构建时的部署前缀，Nginx 转发时剥离前缀。

## 数据与同步

账号在 `mario_auth` schema，各用户关系数据在 `user_<uuid>` schema；公共公司目录、观察快照、分红历史和计划缓存归 `mario_market`，不含个人持仓或交易。服务端根据经过认证的会话选择租户，客户端无法通过指定 user ID 或 schema 越权。数据库账号仅供服务端使用。单账号 JSON、租户 schema 与加密整站备份的版本区别及升级边界见[数据迁移](data-migration.md#备份格式与升级边界)。

可选的网站 **Sign in with ChatGPT** 需先申请 OAuth client ID，在服务器 `.env` 配置后显示入口，详见 [配置、账号关联与申请步骤](chatgpt-sign-in.md)。它只建立 Mario 网页会话；模型执行所用的本机 ChatGPT 授权和 Codex 桥接另在用户电脑配置与配对，见[桥接说明](codex-bridge.md)，不因网站登录而自动获得执行凭据。

业务数据及同步版本在同一事务中更新。快照响应包含 `x-data-revision`，写请求携带 `If-Match`，缺失/过期返回 409；PostgreSQL 账户级 advisory lock 确保多个 API 实例也遵守同一版本顺序。

前端合并版本检查，前台空闲时从 60 秒逐渐放缓到 5 分钟，断网指数退避；只有版本变化才获取快照。有未提交输入时提示核对，并阻止旧表单写入。当前是在线共享数据库模式，离线不能保存，没有后台离线写入队列。

## 备份与密码重置

频繁迁移使用加密 PostgreSQL 快照包，包含所有账号、业务数据及主密钥。打包、SSH 下载、恢复与回退见 [数据迁移指南](data-migration.md)。源码更新不覆盖 `.runtime/native/postgres` 和 `.env`。

管理员可停服后通过服务端命令重置账号密码。不要把密码放进命令参数或 Git；通过 `MARIO_RESET_PASSWORD` 传入，并给二进制提供 `DATABASE_URL`，执行 `--reset-password 用户邮箱`。具体连接可使用配置的外部 URL，或 `postgresql://mario:<URL编码的密码>@127.0.0.1:55432/postgres`。修改密码会撤销该账号全部会话。重置后清除临时环境变量并重新启动服务。

服务端不读取旧单机版的 SQLite 文件；旧资料用标签 `legacy-sqlite-export` 导出后在新账号导入，步骤见 [数据迁移指南](data-migration.md#旧版-sqlite-数据)。

## 高级运行与开发

通常无需手动设置内部参数。服务运行器自动配置 `MARIO_HOST=127.0.0.1`、网页目录、Agent URL 和 Agent 模式。PostgreSQL 当前驱动不启用数据库连接 TLS；外部数据库放在受保护的专用网络/隧道，不直接暴露公网。

用户模型 API Key 在服务端按账号加密，浏览器不能读取原值。用户在“设置”的模型配置中选择接口并填写 HTTP/HTTPS Base URL、模型名称和密钥，无需配置 `MARIO_MODEL_HOSTS`。Base URL 包含供应商要求的路径（如 `/v1`），不包含 `/chat/completions`、`/responses` 或 `/messages` 端点。自建网关可使用自定义端口和 HTTP；请求从服务器发出，本机地址指服务器本机。公网模型连接建议使用 HTTPS。内置登录每账号每分钟最多 10 次，每个 API 进程最多同时执行 2 次密码运算。

```bash
npm ci --prefix client
VITE_BASE_PATH=/mario/ npm run web:build
```

上面的 `web:build` 生成 `client/dist`、复制的 `outputs/web` 和 `outputs/build-manifest.json`。生产 `deploy.sh` 则直接构建并服务 `client/dist`，其来源信息在 `client/dist/build-info.json`；部署不会刷新本地分发包的 manifest。构建、浏览器文件交互与子路径配置见 [Web 构建](client-builds.md)。

本地开发运行 `npm run dev`：脚本在 `.runtime/dev` 创建并启动开发用 PostgreSQL 集群（端口 `MARIO_DEV_PG_PORT`，默认 54329），生成该目录专用的数据库密码与 `MARIO_MASTER_KEY`，再启动 API（`127.0.0.1:4217`）和 Vite；在浏览器注册开发账号即可，退出时一并停止集群。开发机需安装 PostgreSQL 工具，脚本从 `MARIO_TEST_PG_BIN` 或 `pg_config --bindir` 查找。前端可用 `VITE_API_URL` 连接其他临时 API。不要把线上数据库传给开发或集成测试。
