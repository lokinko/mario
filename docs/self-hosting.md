# 自建宿主机服务

生产部署统一使用根目录 [deploy.sh](../deploy.sh)，地址、端口等部署配置集中在脚本顶部并有中文说明；站点 OpenAI 凭据在 `.env` 配置。以下入口为 `https://lokinko.cn/mario/`，源码直接在宿主机编译和运行，systemd 负责后台常驻和开机自启。

```bash
sudo bash deploy.sh --ip 47.106.188.95 --domain lokinko.cn
```

只有 `deploy` 显式同时传入 `--domain`、`--ip` 时才调用独立域名脚本，配置 Nginx、主域名与 www 跳转，并复用双域名证书或通过 HTTP 验证申请。日常 `sudo bash deploy.sh` 不调用它。DNS、证书失败回退及已有网站手工接入见 [服务器部署指南](deploy-madeagents.md)。`.env` 由部署器自动生成，保存运行参数和持久密钥；日常改 `deploy.sh` 后重新部署，密码和主密钥保持不变。真实 `.env` 不提交 Git。

Web 通过 Rust API 读写数据，PostgreSQL 保存独立账号、会话及各用户关系数据，AI Agent 是独立无状态进程。三个服务仅监听回环地址，公网访问由已有 Nginx 提供 HTTPS。Agent 不获得数据库 URL 或主密钥，它仅收到本次授权上下文和模型请求所需凭据。

## 账号与客户端

注册开放，任何用户均可用邮箱和密码直接创建账号，无需邀请码。每个人有独立邮箱、密码与业务数据，可使用站点提供的 OpenAI，也可配置个人模型密钥或本机桥接；邮箱作为账号名，不做邮件验证/邮件重置。通过浏览器访问配置的域名及 `/mario/` 路径。

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

### 站点提供的 OpenAI

管理员在云端部署目录的 `.env` 填写：

```dotenv
OPENAI_API_KEY=填写项目密钥
OPENAI_MODEL=gpt-5.4
```

首次上线此能力需要拉取代码并运行 `sudo bash deploy.sh`；以后只修改这两个值，执行 `sudo bash deploy.sh restart` 即可生效。已有 `.env` 不需要重建，部署准备及升级保留这两个值。文件仅允许服务用户读取（部署器使用 `0600`），不要把真实密钥提交 Git。留空或删除 `OPENAI_API_KEY` 后重启将停用站点模型；填写密钥而省略 `OPENAI_MODEL` 时默认使用上面的模型。模型可用性、额度和消费归该 OpenAI API 项目，连接测试与问答均会调用该项目。

所有已登录用户均可在“设置 → 模型连接 → 调用方式”选择“站点提供的 OpenAI（默认）”。启用站点服务后，未保存明确 `model.source` 选择的账号统一默认使用它，包括只有旧个人模型字段的账号；个人配置和密钥保留，明确选择个人模型或本机桥接后继续按该选择执行，失败不自动切换。没有启用站点服务时仍可使用个人模型；已经选择站点的账号则显示站点不可用。站点地址固定为 `https://api.openai.com/v1`，使用 Responses；普通用户不能改地址、密钥或移除共享密钥。用户的授权资料、分析历史和账本仍按账号隔离。

打开站点模型设置会自动读取当前项目的[官方模型目录](https://developers.openai.com/api/reference/resources/models/methods/list)，筛选候选文本模型并提供下拉选择；`OPENAI_MODEL` 是站点默认型号，不再限制用户只能使用这一个型号。目录只表明项目可见性，不保证 Responses、原生搜索、额度或输出校验可用；保存后可执行连接测试。成功目录在 API 进程缓存 10 分钟，失败退避 30 秒，按凭据摘要隔离并合并并发获取；到期后由下一次设置页请求刷新，不增加后台任务。网络请求最多 10 秒、不跟随重定向、不转发上游原始错误正文，返回体有大小限制。

`GET /api/model-config/models` 需要登录，返回 `models` 与 `defaultModel`，不写账号版本。新选型号必须在服务端目录中；当前已保存型号和管理员默认型号允许在目录不可用或不再返回它们时继续保存，不静默换成其他模型。目录读取失败时页面保留当前/默认选择并可重试，不能把失败目录伪装成成功。站点型号与调用来源由账号仓储在同一事务保存为 `model.server_name`、`model.source`，个人模型字段和密钥不覆盖；目录网络等待期间释放账号锁，保存前复核账号版本，发现并发修改返回冲突并保留草稿。

共享凭据由 API 进程启动时读取，不写入账号设置，不出现在配置读取、预览、分析结果或个人 JSON 备份中；独立 Agent 只在受认证的单次冻结作业中收到执行所需凭据，启动时不继承站点密钥。前端及 Rust 构建也不继承该环境变量。来源和型号偏好与其他模型设置一样不进入个人 JSON 备份；整站数据库备份保留选择，但现有加密迁移包只携带数据库和主密钥，不包含这两个站点环境值；迁移时需安全转移 `.env` 或在目标服务器重新填写，恢复脚本保留目标已有的站点配置。无需新增 schema 或备份版本，停用站点模型时原选择显示不可用，用户可切换个人模型；已有个人密钥保留。

配置只决定模型来源，不扩大问答资料授权或允许免登录调用。所有问答仍使用[既有数据边界](ai-data-boundary.md)及输出校验。服务端使用环境变量保管密钥遵循 [OpenAI 官方认证说明](https://developers.openai.com/api/reference/overview)。

### 个人模型

接入 OpenAI 官方 API 时，在“设置 → 模型连接”选择 `OpenAI Responses`，Base URL 填 `https://api.openai.com/v1`，模型名称填写该 API 项目可用的模型 ID，API Key 填该项目的密钥。保存后执行连接测试，再到问答页预览授权资料并发送问题。连接测试只验证普通生成；完整问答还须通过结构与引用校验，联网查找权限也需单独验证。一次真实调用结果见 [OpenAI API 接入验证记录](audits/openai-api-smoke-20261116.md)，其中的模型和耗时仅代表当次测试。

```bash
npm ci --prefix client
VITE_BASE_PATH=/mario/ npm run web:build
```

上面的 `web:build` 生成 `client/dist`、复制的 `outputs/web` 和 `outputs/build-manifest.json`。生产 `deploy.sh` 则直接构建并服务 `client/dist`，其来源信息在 `client/dist/build-info.json`；部署不会刷新本地分发包的 manifest。构建、浏览器文件交互与子路径配置见 [Web 构建](client-builds.md)。

本地开发运行 `npm run dev`：脚本在 `.runtime/dev` 创建并启动开发用 PostgreSQL 集群（端口 `MARIO_DEV_PG_PORT`，默认 54329），生成该目录专用的数据库密码与 `MARIO_MASTER_KEY`，再启动 API（`127.0.0.1:4217`）和 Vite；在浏览器注册开发账号即可，退出时一并停止集群。开发机需安装 PostgreSQL 工具，脚本从 `MARIO_TEST_PG_BIN` 或 `pg_config --bindir` 查找。前端可用 `VITE_API_URL` 连接其他临时 API。不要把线上数据库传给开发或集成测试。
