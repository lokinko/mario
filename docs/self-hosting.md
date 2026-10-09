# 自建宿主机服务

生产部署统一使用根目录 [deploy.sh](../deploy.sh)，所有日常配置集中在脚本顶部并有中文说明。默认入口为 `https://www.madeagents.ai/mario`，源码直接在宿主机编译和运行，systemd 负责后台常驻和开机自启。

```bash
sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
```

完整的依赖安装、非 root 服务账号、Nginx 接入、升级和验收步骤见 [服务器部署指南](deploy-madeagents.md)。不再维护 Docker/Caddy 或独立 Web 启动入口。`.env` 由部署器自动生成，保存运行参数和持久密钥；日常改 `deploy.sh` 后重新部署，密码和主密钥保持不变。真实 `.env` 不提交 Git。

Web 与 Tauri 客户端共用 Rust API，PostgreSQL 保存独立账号、会话及各用户关系数据，AI Agent 是独立无状态进程。三个服务仅监听回环地址，公网访问由已有 Nginx 提供 HTTPS。Agent 不获得数据库 URL 或主密钥，它仅收到本次授权上下文和模型请求所需凭据。

## 账号与客户端

服务器 `.env` 的 `MARIO_REGISTRATION_KEY` 是注册邀请码。每个人有独立邮箱、密码与模型密钥；邮箱作为账号名，不做邮件验证/邮件重置。轮换邀请码不影响已有账号。网页和新客户端可使用 `https://www.madeagents.ai/mario`；旧客户端安装包需要更新才能接受部署路径。

会话保存在当前窗口的 `sessionStorage`，关闭窗口后需重新登录；服务端会话有效期 7 天，退出会撤销当前会话。网页、API 和资源统一使用构建时的部署前缀，Nginx 转发时剥离前缀。

## 数据与同步

账号在 `mario_auth` schema，各用户关系数据在 `user_<uuid>` schema。服务端根据经过认证的会话选择租户，客户端无法通过指定 user ID 或 schema 越权。数据库账号仅供服务端使用。

业务数据及同步版本在同一事务中更新。快照响应包含 `x-data-revision`，写请求携带 `If-Match`，缺失/过期返回 409；PostgreSQL 账户级 advisory lock 确保多个 API 实例也遵守同一版本顺序。

前端合并版本检查，前台空闲时从 60 秒逐渐放缓到 5 分钟，断网指数退避；只有版本变化才获取快照。有未提交输入时提示核对，并阻止旧表单写入。当前是在线共享数据库模式，离线不能保存，没有后台离线写入队列。

## 备份与密码重置

频繁迁移使用加密 PostgreSQL 快照包，包含所有账号、业务数据及主密钥。打包、SSH 下载、恢复与回退见 [数据迁移指南](data-migration.md)。源码更新不覆盖 `.runtime/native/postgres` 和 `.env`。

管理员可停服后通过服务端命令重置账号密码。不要把密码放进命令参数或 Git；通过 `MARIO_RESET_PASSWORD` 传入，并给二进制提供 `DATABASE_URL`，执行 `--reset-password 用户邮箱`。具体连接可使用配置的外部 URL，或 `postgresql://mario:<URL编码的密码>@127.0.0.1:55432/postgres`。修改密码会撤销该账号全部会话。重置后清除临时环境变量并重新启动服务。

旧 SQLite 资料可由服务端 `--export-data` 导出，登录新账号后通过“账号与数据”导入；旧账号/会话/模型密钥不随业务 JSON 导入。导出时使用 `MARIO_DATA_DIR` 指向旧资料目录，详情可运行二进制 `--help`。

## 高级运行与开发

通常无需手动设置内部参数。服务运行器自动配置 `MARIO_HOST=127.0.0.1`、网页目录、Agent URL 和 Agent 模式。PostgreSQL 当前驱动不启用数据库连接 TLS；外部数据库放在受保护的专用网络/隧道，不直接暴露公网。

用户模型 API Key 在服务端按账号加密，浏览器不能读取原值；配置和调用经 HTTPS。添加自定义模型主机前核对其信任边界。内置登录每账号每分钟最多 10 次，每个 API 进程最多同时执行 2 次密码运算。

```bash
npm ci --prefix client
npm run windows:build    # Windows/MSVC/WebView2
npm run desktop:build    # 在目标桌面系统构建
```

开发前端可通过 `VITE_API_URL=http://127.0.0.1:4217/api` 连接临时多用户 API；无此变量的 Vite 开发模式保留隔离的 SQLite 调试路径，不是生产服务。不要把线上数据库传给集成测试。
