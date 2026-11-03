# ChatGPT OAuth 与 Responses 实现复核（2026-11-03）

## 范围与结论

复核 `6696605` 引入的本机 ChatGPT 官方授权与 Responses 桥接。实现采用官方公开的动态客户端注册流程，独立于 Codex CLI 登录；不读取 Codex `auth.json`，不将其令牌当作 API Key。实现方向正确，但自动化验收不能证明真实账号已完成授权或具备订阅调用权限。

核对来源为 OpenAI 官方文档：[注册和登录](https://developers.openai.com/siwc/token-sharing-open-source/sign-in)、[模型和推理](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference)、[令牌参考](https://developers.openai.com/siwc/token-sharing-open-source/token-reference)。公开接入范围与预览限制仍按[本机桥接说明](../codex-bridge.md)执行。

## 已核对的边界

- 首次使用 `dynamic_agent_client`，保存回调返回的 issued client ID；同一主机复用 host ID，返回登录复用注册，新增账号显式创建。使用 PKCE、state、nonce 和已签名 ID token；按 subject 与 client ID 区分注册，不按邮箱合并。
- 凭据仅保存在当前系统用户配置目录，使用受保护权限、跨进程锁和原子替换；在线 HTTP、业务仓储和 Agent 不依赖本机凭据模块。刷新替换令牌，退出先清除本机令牌，再尝试远端撤销；不记录上游敏感错误正文。
- 只向固定官方 models 与 Responses 端点发送访问令牌，不跟随重定向；请求使用 `instructions`、`store:false`、`stream:true`，仅在授权搜索时启用 `web_search`。只有完整 `response.completed` 才返回成功，断流、失败和不完整输出不会保存为完整结果，不自动重放推理。

## 本次修复

原 worker 固定注册 ID 与会话，并定期检查会话，但 Provider 的取令牌接口只接收注册 ID。如果重新登录发生在 worker 检查之后、取令牌之前，旧 worker 可能取得同一注册的新授权令牌。结果阶段的会话检查无法阻止这次上游请求。

沿用[架构说明](../architecture.md)第 4 节本机执行模块和第 7 节状态归属：worker 把固定会话传入本机 Provider；模型目录和推理都在凭据锁内、读取或刷新前核对同一会话。旧会话失败时不返回令牌；常规刷新保留会话。新增回归覆盖重新登录替换、退出和订阅权限禁用。无数据库迁移，无桥接协议或业务事务调整，删除原来只按注册 ID 取令牌的接口。

## 验证记录

- `npm run test:chatgpt`：启动参数 2 项、Rust OAuth／Responses 27 项通过，包含 Windows 凭据 DACL 与原子替换验证。
- `npm run check:architecture`：通过，包含 8 项架构检查测试。
- 前端构建、Prettier 检查通过；桥接设置和模型选择 2 个测试文件、7 项测试通过；完整前端 39 个文件、267 项测试通过。
- 桥接取消、撤销、隔离、失败分类等 4 项单元测试及 worker 安全 URL 1 项测试通过。
- `cargo fmt --check`、全部 target 的 Clippy（`-D warnings`）、Rust 构建：通过。
- `npm run test:server`：缺少本机 PostgreSQL 的 `pg_config`／`MARIO_TEST_PG_BIN`，在创建一次性集群前失败；没有使用开发或生产数据库替代。
- `npm test`：用本机 Git Bash 和指定 Python 虚拟环境执行，架构、启动参数、QA 评分、184 项 benchmark 测试、部署脚本、前端全部测试、Rust 格式／Clippy／构建通过，随后因上述 PostgreSQL 工具缺失失败。未执行数据库 Rust 测试、HTTP／Web／Codex 桥接全链路及后续财务集成验收，不能视为完整测试通过。
- 全部验证结束后核对目录绝对路径和进程占用，执行 `cargo clean --manifest-path server/Cargo.toml --target-dir server/target`，移除本次产生的 3.7 GiB 编译产物并确认目标已删除或为空。`.runtime/` 没有本次遗留临时目录，保留原有 `benchmark/` 与 `financial-audit/` 数据。

未执行真实浏览器登录、订阅额度调用或部署；测试只使用隔离临时数据和测试专用密钥。真实 OAuth 授权和连接测试仍需账号持有人在本机完成。
