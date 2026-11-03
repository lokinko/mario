# 网页使用个人电脑的 ChatGPT / Codex

Mario 共用一条账号配对桥接，支持两种本机执行方式：

| 方式 | 本机命令 | 登录与模型调用 | 本机要求 |
| --- | --- | --- | --- |
| ChatGPT 官方账号授权 | `npm run chatgpt:bridge` | Mario 发起 Sign in with ChatGPT；本机通过官方 Responses API 使用已授权订阅 | 同版 Mario 源码、Node.js、Rust；不需要安装 Codex |
| 已登录的 Codex | `npm run codex:bridge` | Codex 自己保管和刷新登录；Mario 调用本机 Codex App Server | 上述工具，以及已登录的 Codex |

调用链：Mario 问答 → 已配对账号的临时请求通道 → 个人电脑上的桥接进程 → 官方 Responses API 或本机 Codex App Server。电脑主动通过 HTTPS 长轮询接收任务，不需要穿透 NAT，不对外开放本机服务。**ChatGPT 官方授权时例外：临时回调监听器只绑定 `127.0.0.1`，用于接收本机浏览器的 OAuth 回调，不是公网桥接入口。**

两种方式的登录凭据都留在本机，不上传到 Mario 服务器。网页沿用同一个 `codex-bridge` Provider 契约；无需为了切换执行方式修改服务器模型地址或粘贴令牌。

## 部署和配对

1. 更新服务器源码，在项目目录执行 `sudo bash deploy.sh`。脚本构建前后端并验证 `api/server` 的 `codexBridge: 1`；不需要给服务器安装 Codex，也不在服务器登录 ChatGPT。
2. 执行 `bash deploy.sh check` 验证公网入口、网页资源及桥接鉴权。`/mario/` 反向代理覆盖桥接接口，不需要 WebSocket 配置或额外公网端口。
3. 在个人电脑准备同版 Mario 源码、Node.js、Rust，按下一节选择一种本机执行方式。
4. 登录 Mario，在设置的模型接口选择 **本机桥接（ChatGPT 登录 / Codex）**，点击 **生成配对码**。
5. 在电脑的 Mario 项目目录启动所选桥接，按提示粘贴网页生成的 Mario 配对码并回车。配对码通过标准输入传入，不作为命令行参数或环境变量保存；不要粘贴 ChatGPT 密码或 OAuth 令牌。
6. 保持窗口运行，网页显示在线后点击 **启用本机模型**，选择模型并 **保存配置**，再点击 **测试已保存连接**。连接测试会实际发起推理并使用账号额度。
7. 正常使用问答和流式回复。关闭电脑或桥接进程后问答会提示不可用。**撤销配对**会中止当前调用并阻止该进程接收后续任务；它不等于退出本机 ChatGPT 或 Codex 账号。

### 方式一：ChatGPT 官方授权（无需 Codex）

```bash
npm run chatgpt:bridge
# 其他 Mario 站点；地址必须包含实际部署前缀
npm run chatgpt:bridge -- https://your-host/mario/
# 本地开发可用实际本机端口
npm run chatgpt:bridge -- http://127.0.0.1:4217/
```

默认站点为 `https://www.madeagents.ai/mario/`。没有已选择的本机 ChatGPT 账号时，桥接会自动发起官方浏览器登录与授权，然后读取该账号的模型列表并等待 Mario 配对码。已有本机账号时复用该账号；**只在浏览器登录 ChatGPT，并不等于已授权 Mario 使用订阅。**

首次授权使用 OAuth Authorization Code + PKCE、`state` 和 OIDC `nonce`，验证 ID token 和实际授予的权限。身份 scopes 为 `openid profile email`；订阅使用 scopes 为 `offline_access resource.invoke chatgpt.tokens.use.direct`，资源为 `https://api.openai.com/v1`。缺少 `chatgpt.tokens.use.direct` 时不发起推理，不能用“已登录”替代订阅授权。应用只获取授权身份和调用权限，不读取 ChatGPT 的历史对话或其他账号上下文。

本机管理命令：

```bash
# 登录或重新授权
npm run chatgpt:auth -- login
# 新增一份账号 / 工作区注册
npm run chatgpt:auth -- login --new
# 为已有注册重新授权；替换示例中的 registration-id
npm run chatgpt:auth -- login --account <registration-id>
# 查看本机注册与当前选择
npm run chatgpt:auth -- status
# 切换后续桥接启动使用的注册
npm run chatgpt:auth -- select <registration-id>
# 退出当前或指定本机注册
npm run chatgpt:auth -- logout
npm run chatgpt:auth -- logout <registration-id>
```

注册 ID 是本机标识，不是访问令牌。示例中的 `<registration-id>` 必须替换，不能原样作为终端命令使用。每个账号 / 工作区的注册与令牌分开保存；每台运行电脑使用稳定的 host ID。账号切换只改变后续启动的选择，**已运行的桥接固定使用启动时的本机注册，不会悄悄跟随当前账号切换**。要更换执行账号，请停止旧进程，在网页重新配对后启动新桥接；不共享正在运行的账号通道。

Mario 只向固定官方 `GET https://api.openai.com/v1/models` 和 `POST https://api.openai.com/v1/responses` 发送推理授权；不跟随重定向，不调用 ChatGPT 的 `backend-api`，不把 OAuth 令牌填进普通 API Key 配置，不自动切换到其他计费方式。访问及刷新令牌由本机授权模块保存和刷新，不进入网页、服务器数据库、Mario 账号备份或日志。凭据位于系统用户配置目录的 `mario/chatgpt/accounts.json`（Windows 通常为 `%APPDATA%` 下、Linux 为 `~/.config` 下、macOS 为 `~/Library/Application Support` 下），使用当前系统用户专属权限；不要上传、提交或复制到服务器。刷新通过跨进程锁串行化并原子替换访问及轮换刷新令牌。`logout` 先清除本机令牌并使运行中的桥接取消任务，再尝试远端撤销；撤销未确认会明确提示到 ChatGPT 设置断开应用。退出保留注册和 host ID，普通 `login` 复用已选择注册，不重复动态注册。若只获得身份权限但未开启订阅使用，注册会保留并显示禁用状态；使用 `login --account <registration-id>` 重新同意完整授权后才允许推理。

### 方式二：使用已有 Codex 登录

```bash
codex login
npm run codex:bridge
# 其他站点
npm run codex:bridge -- https://your-host/mario/
```

仅浏览器登录 ChatGPT 不等于 Codex CLI 已登录。此方式继续使用 Codex 自己的登录与刷新机制，不读取或上传 `auth.json`，不把 Codex 的凭据导入 Mario 官方 OAuth 注册。

如果 CLI 安装在自定义位置，可设置 `MARIO_CODEX_BIN` 为原生 `codex.exe` / `codex` 的绝对路径。桥接不执行 `.cmd` 或 shell 命令来传递提示词。Codex 模型由本机 `model/list` 检测；成功的连接测试才确认该模型能实际推理。

## 模型与官方预览限制

本机在线后，在 **模型名称** 下拉框选择模型并点击 **保存配置**。后续问答和连接测试使用所选模型；正在执行的请求继续使用启动时的模型。

- ChatGPT 官方方式读取当前授权账号的 `/v1/models`，按服务端顺序显示 `models` 数组中 `visibility: "list"` 的条目，使用 `display_name` 展示、`slug` 调用。列表第一项作为桥接默认选择，不保证是 ChatGPT 网页的默认模型。
- Codex 方式显示本机返回的名称、说明和默认标记；列表可能来自客户端目录，不代替实际权限验证。
- 两种方式的具体模型、联网搜索和额度均受账号、订阅与工作区政策限制，不能保证与 ChatGPT 网页菜单一致。列表可见不保证推理成功。
- 桥接只上传非敏感模型元数据，网页每 3 秒刷新连接和列表状态。更换账号、更新 Codex 或希望更新模型目录时，重新配对并重启相应桥接。断线不清除已保存模型；再次启用时保留仍在目录中的选择。

ChatGPT 订阅调用遵循官方预览约束：每次 Responses HTTP 请求固定 `store: false`、`stream: true`，上下文由本次 `input` 数组传入，系统指令归 `instructions`，不发送显式 system 消息，也不使用服务器历史续接。不会发送 `temperature`、`max_output_tokens`、`previous_response_id`、`background` 等此路径不支持的参数。仅在用户授权联网搜索时提供 `web_search`；不开放终端、文件、MCP、子代理或其他客户端工具，也不启用此路径不支持的图片生成、文件搜索、Code Interpreter 等托管能力。

**适用范围不是无限制的订阅 API。** OpenAI 当前公开说明面向开源及本地托管应用；付费或远程托管应用需按官方流程另行申请和评估。将用户令牌保留在个人电脑是 Mario 的安全边界，**不代表一个公网、多用户或商业化 Mario 服务自动满足上游接入资格**。订阅可能有应用级或账号级限额；看到使用限制时到 [ChatGPT 设置 → Usage](https://chatgpt.com/settings/usage) 查看，不推断整个订阅已耗尽或具体恢复时间。

官方资料：[ChatGPT plan usage 概述](https://developers.openai.com/siwc/token-sharing-open-source.md)、[注册和登录](https://developers.openai.com/siwc/token-sharing-open-source/sign-in.md)、[模型与推理](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference.md)、[预览限制](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations.md)、[错误与恢复](https://developers.openai.com/siwc/token-sharing-open-source/errors-and-recovery.md)。上述范围和能力以 OpenAI 当时的实际许可与账号能力为准。

## 配对边界与生命周期

- 配对码是随机 256 位的 Mario 通道凭证，不是 ChatGPT 登录令牌；请只粘贴进自己电脑的桥接进程。网页仅临时显示，服务端通道只保存摘要。普通账号请求不能指定其他账号的桥接 ID。
- 配对等待 10 分钟后过期；已连接的配对最长有效 12 小时。重新配对、撤销、服务重启都会使旧通道失效；重启本机进程也需重新配对。网络短断可以由同一进程重连，已经开始的推理不会自动重放。
- 面向个人自用、单 API 进程部署，通道保存在进程内存中。多副本负载均衡需要后续共享通道或固定路由支持，不能直接扩容使用。
- 每个配对同时执行一个模型请求；另一个请求得到忙碌提示。正常执行经过 Mario 的上下文冻结、结构化校验和结果保存流程，只有完整成功的回答入库。
- 本机每 500ms 发送结果片段或心跳。浏览器停止生成或撤销配对后，服务端取消请求，本机下一次提交发现取消后释放官方 HTTP 请求或 Codex 子进程；网络不可用时最多等待当前桥接 HTTP 请求的 25 秒期限。
- 本机进程仅接收模型、文本消息和是否联网搜索，不接收任意命令、工作目录或工具配置。Codex 使用只读临时会话并关闭文件、终端、MCP、应用和子代理工具；官方 Responses 路径没有这些本机工具的执行入口。
- 这是用户个人电脑的执行桥接，不是在托管服务器共享个人 ChatGPT 或 Codex 登录，也不为其他 Mario 账号借用订阅。

## 超时、失败与重试

联网搜索和格式修复会增加模型调用，单次调用时限不代表整个问答的时限。完整流程在服务端最多执行 25 分钟，独立 Agent 的 HTTP 通道最多等待 26 分钟，网页总等待上限为 30 分钟。SSE 每 15 秒发送心跳；网页连续 60 秒没有收到任何状态、内容或心跳才判定连接停滞。超过流程上限或用户停止时会取消执行，不保存未完成结果。

ChatGPT 官方方式每次调用最多 180 秒（包含取得可用访问令牌的等待），**不自动重试推理请求**。只有收到 `response.completed` 且结果状态完整才视为成功；即使已显示草稿，随后出现失败、不完整、断流或缺少终止事件，也不会保存为完整回答。用量限制、权限不足、暂时不可用分别按安全错误分类处理，不透传上游错误正文或令牌，不因失败改用 API Key 计费。

Codex 单次调用（含全部重试）最多 180 秒。在尚未产生任何助手内容时，已分类的临时繁忙、限流或连接故障最多自动重试两次，使用递增等待与随机抖动。网页及桥接终端显示恢复进度，调用统计包含实际尝试次数。Codex 自身仍在恢复连接时等待它完成，不叠加重新执行；它已耗尽内部重试时也不重复启动一轮重试。登录失效、用量限制、上下文上限、请求拒绝、未知错误，以及已有内容后的失败均不会自动重新推理。

普通 OpenAI/Anthropic API 的重试位于 [`native_provider.rs`](../server/src/ai/native_provider.rs) 的 `send_request`；Codex 重试位于 [`codex.rs`](../server/src/ai/codex.rs) 的 `retry_turn`；官方 ChatGPT HTTP 适配位于 [`chatgpt.rs`](../server/src/ai/chatgpt.rs)。网页不自动重发整个问答，以免重复生成或写入历史。桥接等待任务时遇到网络故障或临时 HTTP 错误会自动重连，这不等于重放已开始的模型请求。

桥接协议变化时需要同时部署服务器和更新本机源码，然后在网页重新配对并重启所选桥接。仅刷新网页不能更新已运行的本机程序。

## 自动化验收

- 本机授权会话在模型目录和推理取令牌时再次校验，且与读取、刷新共用凭据锁。重新登录后，即使注册 ID 没有变化，旧 worker 也不能借用新会话；必须重启并重新配对。常规刷新保留会话，切换默认账号不改变已运行的 worker。
- `npm run test:chatgpt` 是无需数据库、真实登录和模型额度的本机回归入口，包含启动参数安全、本机 RSA/JWK 签名和 OIDC 声明、OAuth 回调拒绝与单次消费、部分授权、刷新错误分类、会话与凭据权限、流式 Responses 解析；全部使用临时数据或明确的测试专用密钥。
- `cargo test --manifest-path server/Cargo.toml --lib ai::chatgpt::tests` 运行纯 HTTP 载荷与 SSE 解析回归：分片 UTF-8、流中失败、完整终止、引用与搜索授权、模型目录及大小边界；不登录、不访问真实模型、不消耗额度。
- 前端 `npm test --prefix client` 覆盖两种桥接命令说明、配对安全、在线后启用、撤销、模型保存和卸载期间的过期响应保护。
- `npm run test:codex-bridge -- --protocol-fixture` 编译只读的假 App Server（不访问登录或真实模型），通过真正的 Codex 本机桥接进程验证 503 恢复、重试状态、联网与结果保存，以及用量限制、已有输出和原生恢复不重放。客户端虚拟时钟回归覆盖带心跳的六分钟流程、断线和总时限；服务端虚拟时钟回归覆盖超过五分钟的成功执行，以及总超时取消和不落库。
- `cargo test --manifest-path server/Cargo.toml bridge` 检查隔离、流式结果、取消、撤销、过期和安全 URL。`npm run test:codex-bridge` 自动创建一次性 PostgreSQL 集群，用模拟本机工作进程验证多账号 HTTP 全链路：账号隔离、独立 Agent 绕过、SSE 保存、浏览器取消和重启失效；结束后删除整个集群，不接触生产数据库。加 `-- --live` 会启动真实本机 Codex、消耗本机账号额度并验证连接及流式问答。

自动化解析及模拟桥接验收不等于完成真实 OAuth 授权或订阅推理，也不能替代公网 Nginx、线上 PostgreSQL 和真实账号部署验收。真实登录和消耗额度的连接测试需由用户明确执行。

2026-11-03 的实现复核与本机验证记录见 [OAuth 与 Responses 复核](audits/chatgpt-oauth-2026-11-03.md)。
