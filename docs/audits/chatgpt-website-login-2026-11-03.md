# 网站 ChatGPT 身份登录验收（2026-11-03）

## 交付范围

接入官方网站身份登录：同源 POST 发起、整页授权跳转、PKCE/state/nonce、服务端签名校验、数据库一次性事务、明确的账号关联和短期会话交接。只请求身份 scopes，不接入订阅模型权限，也不依赖 Mario 配对码或本机程序。未获批网站 OAuth client ID 时隐藏入口。

短期 OAuth 事务和会话交接分表保存，只保留 state、浏览器标识及会话／交接凭证的摘要；PKCE verifier 和 nonce 只用于短期登录事务。OpenAI 原始令牌不持久化，不向浏览器返回。client secret 只从服务器配置读取，运行器不把它传给无状态 Agent。

已有邮箱不自动合并；关联绑定仍有效的 Mario 会话。同一个外部身份不能覆盖另一用户的关联。新账号、身份映射和会话签发在账号事务内完成，账号业务 schema 与财务数据路径不变。登录后继续使用现有 Mario 自有 Bearer 会话。

## 验证

- 网站 OIDC 单元测试 2 项通过：配置、Cookie、精确回调、官方端点、真实测试 RSA 签名、issuer/audience/nonce/expiry/iat 和多受众 azp。
- HTTP 回调解析单元测试 1 项通过：重复字段、过大查询、缺失／重复／无效浏览器 Cookie 拒绝。
- 账号前端相关 3 个测试文件、21 项测试通过，包含 StrictMode 一次性交接、账号切换、取消、异常授权地址、返回会话校验和未配置隐藏入口。
- 全部前端 41 个文件、275 项测试、构建和 Prettier 格式检查通过；Rust 格式、全部 target 的 Clippy（`-D warnings`）和构建通过；架构检查 8 项通过。部署脚本检查通过，包含 OAuth 回调独立 location、查询日志关闭和代理路径校验。
- 已执行 `npm run test:server`，因缺少本机 PostgreSQL 的 `pg_config`／`MARIO_TEST_PG_BIN`，在建立一次性集群前失败。新增 5 项数据库测试（身份隔离、显式关联、登录事务单次消费、会话交接、旧账号迁移）已编译，尚未运行；HTTP 全链路也未运行。没有连接开发或生产数据库替代验收。
- 最终 `npm test` 已用 Git Bash 与指定 Python 虚拟环境运行，前述架构、QA、部署、全部前端和 Rust 静态／构建门禁通过，随后在一次性 PostgreSQL 工具检测处失败；后续数据库、HTTP／Web／桥接和财务集成验收未执行，不能声称完整测试通过。
- 全部验证结束后确认仓库内 `server/target` 绝对路径及没有编译／服务进程使用，执行 `cargo clean` 移除 3.7 GiB 编译产物并确认目录已删除。`.runtime` 无本次遗留临时目录，保留原有 financial-audit 数据。

没有真实网站 client ID，未执行真实 OpenAI 登录。未提交官方申请、未部署。申请步骤、生产回调地址及可调整的英文产品说明见[网站登录说明](../chatgpt-sign-in.md)。OpenAI 当前的获批范围以官方实际回复为准。
