# 网站 Sign in with ChatGPT

## 当前范围

这是 Mario 网站的账号登录方式：用户从登录页点击 **Continue with ChatGPT**，整页跳转到 OpenAI 完成认证，再自动回到 Mario。已登录用户可在账号设置点击 **关联 ChatGPT 登录**，以后用该身份登录同一个 Mario 账号。无需安装本机程序或使用 Mario 配对码。

本功能只请求 `openid profile email`，不申请订阅额度、不调用模型、不读取 ChatGPT 对话或记忆。模型提供方和本机 Codex／ChatGPT 执行能力保持原有配置；网站身份登录与这些模型执行方式独立。

**代码接入不等于 OpenAI 已开放该网站的登录资格。** 当前官方要求申请 OAuth client ID 和注册精确回调地址；网站登录仍面向获批合作伙伴。未配置时登录页和账号设置隐藏相关入口，不影响邮箱密码登录。

## 申请 client ID 与 ChatGPT plan usage

官方入口：[Request a client ID](https://developers.openai.com/siwc/request-client-id)，该页指向 [Sign in with ChatGPT interest form](https://openai.com/form/sign-in-with-chatgpt-interest/)。截至 2026-11-03，表单包含工作邮箱、姓名、公司、网站、可选职位、所需能力和产品说明。公司／主体信息须按实际情况填写。

如果同时希望使用订阅额度，能力项选择 **Sign in and ChatGPT plan use for AI requests**，而不只选择身份登录。在产品说明中分别描述当前身份登录和希望后续启用的模型请求，提交后等待 OpenAI 的接入评估与注册信息；公开页面没有给出保证获批、费用或审核时限。填写或提交表单不会自动授予能力。

可据实调整的 Mario 产品说明草稿：

> Mario is a personal investment research workspace available at https://www.madeagents.ai/mario/. It stores each user's portfolio and research records in an isolated account. We would first like to support ChatGPT identity sign-in and explicit account linking. We also request evaluation for ChatGPT plan usage for user-initiated research questions through the Responses API. Mario does not execute trades. Please advise on the approved hosted-app architecture, scopes, credential handling, and usage controls for this deployment.

申请时准备以下技术信息：

- 网站：`https://www.madeagents.ai/mario/`（如使用其他域名，请替换为实际部署地址）。
- 生产回调：`https://www.madeagents.ai/mario/api/auth/openai/callback`，必须与 OpenAI 注册值逐字一致。
- 当前身份 scopes：`openid profile email`。
- 注册的 token-endpoint 认证方式：`none` 或 `client_secret_basic`，按 OpenAI 下发值配置。
- 账号策略：以 issuer、client ID、subject 映射；邮箱相同不自动合并，原账号持有人需先登录后明确关联。
- 单独说明未来是否由远程托管服务执行模型请求。官方开源／本机订阅文档不自动覆盖远程托管产品。

网站身份 client ID 的批准不代表 ChatGPT plan usage 同时获批；即使同时申请，也应以 OpenAI 实际授予的注册能力和用户授权为准。此次没有替用户提交申请。

## 获批后的服务器配置

把下列值添加到服务器真实 `.env`，不要提交真实凭据：

```dotenv
MARIO_OPENAI_CLIENT_ID=oaiapp_替换为签发值
MARIO_OPENAI_REDIRECT_URI=https://www.madeagents.ai/mario/api/auth/openai/callback
MARIO_OPENAI_TOKEN_AUTH_METHOD=none
MARIO_OPENAI_CLIENT_SECRET=
```

若 OpenAI 签发 confidential client，则将认证方式改成 `client_secret_basic` 并在服务器填入 secret。认证方式必须与注册一致；不能为缺失或无效 secret 自动退回 public client。部署运行器仅将上述配置传给 API，不传给 Agent。日常 `deploy.sh` 更新保留已存在的 `.env` 项。

沿用根目录 `deploy.sh` 更新并重启服务器；此次代码交付不会自动部署。将新生成的 `.runtime/native/nginx.conf` 中 OAuth 回调的精确 location 一并更新到现有 HTTPS server，再执行 `nginx -t` 并重载；该 location 关闭授权回调访问日志。检查同源 `GET /mario/api/auth/openai/config` 返回 `{"enabled":true}` 后，登录页才显示按钮。

开发环境需同源访问构建后的网页与 API，或让开发代理正确转发 API 和 Cookie；分离端口的跨源 `VITE_API_URL` 客户端不显示该入口。本机 HTTP 仅允许 localhost／回环地址，生产要求 HTTPS。更改前缀时应重新注册回调并同步配置。

## 登录、关联与凭据边界

1. 网页向同源 API 的 `/auth/openai/start` 发起 POST，并跳转到返回的官方授权地址。关联模式还必须携带当前有效的 Mario 会话；服务端校验请求 Origin。
2. API 从官方 discovery 获取并校验授权端点；生成 state、nonce、PKCE verifier 和随机浏览器标识。短期事务保存在 PostgreSQL 的 `mario_auth.openai_login_attempts`，state 和浏览器标识仅存摘要。HttpOnly、SameSite=Lax、生产 Secure 的临时 Cookie 绑定浏览器，10 分钟过期。
3. 回调原子删除并返回同时匹配 state、浏览器、client ID、回调和有效期的事务；重复、过期或错误浏览器拒绝。用户拒绝也消耗匹配事务。签名校验包括 RS256、公钥、issuer、audience、nonce、到期时间、iat 和多受众 azp。
4. API 仅在服务端交换授权码，不持久化原始 OpenAI ID token；不要求 identity-only 响应提供 access／refresh token，也不保存这些令牌。所有上游端点限定官方 HTTPS 域名，不跟随重定向，不自动重试授权码交换。
5. 首次登录仅为提供已验证邮箱的新身份创建账号，生成不可知的密码值，默认通过 ChatGPT 登录；若需要邮箱密码登录，由现有管理员重置密码流程设置。若该邮箱已有 Mario 账号，则拒绝自动合并，提示先用原方式登录并关联。已关联身份按稳定 subject 登录，外部邮箱变化不会重定向到另一个 Mario 账号。关联时重新检查原 Mario 会话是否仍有效，已经关联其他账号的身份不能被覆盖。
6. 回调只设置 HttpOnly 短期交接 Cookie 并 303 返回固定 Mario 根页面，URL 仅含结果状态，不含令牌。`mario_auth.openai_login_results` 保存交接摘要、浏览器摘要及会话摘要，120 秒有效。网页向同源 `/auth/openai/complete` POST，原子消费交接后取得 Mario 自己的会话；React StrictMode 复用同一次请求，不重放交接。浏览器只保存发起时的 Mario 账号上下文，发生账号切换或超时则拒绝接纳返回结果。完成后删除短期 Cookie 和上下文。
7. 网站继续使用 Mario 现有可撤销 Bearer 会话和账号切换机制。OpenAI ID token、授权码、PKCE verifier 和 client secret 不返回 JavaScript；交接仅返回 Mario 自有会话。这里没有将所有 Mario 登录方式迁移成 HttpOnly 长期会话。

回调响应禁缓存、禁引用来源传递和嵌入；请求日志仅记录 method 和 path，不记录含授权码的查询串。Nginx／外层网关也不得为该回调记录查询串：生成的 Nginx 精确回调 location 设置了 `access_log off`；若使用其他网关或自定义日志，也应禁用该回调的查询串记录。不按原样记录 OAuth 上游错误正文。

退出 Mario 撤销 Mario 会话；身份映射保留，以便下次登录。此版本尚未接入 OpenAI 的账户安全通知，因此不宣称在 OpenAI 端登出会同步撤销所有 Mario 会话。

## 数据迁移与验证

启动时在 `mario_auth` 添加 `external_identities`、`openai_login_attempts`、`openai_login_results` 及索引，不改已有 users／sessions 或用户业务 schema。账号创建／关联／会话签发在单一事务内，短期会话交接另行落库并一次性消费；失败回滚相应事务，OAuth 事务或交接已消费时需重新登录。新增表可重复初始化，原邮箱密码登录继续有效；回退旧程序不会删除映射或用户数据。数据库备份包含这些账号级映射，恢复按现有宿主机备份流程执行。

测试包含签名与声明拒绝、配置和官方端点约束、重复回调字段、浏览器绑定、事务／交接单次消费、身份隔离、邮箱冲突、关联会话撤销和旧密码保持，以及同源限制、取消、过期响应、账号切换和 StrictMode 一次性交接。数据库测试必须通过一次性 PostgreSQL 集群运行；未运行的检查不视为通过，没有真实 client ID 时自动化不能替代官方端到端登录验收。本次执行结果见[验收记录](audits/chatgpt-website-login-2026-11-03.md)。

官方资料：[网站身份登录](https://developers.openai.com/siwc/website)、[Quickstart](https://developers.openai.com/siwc/quickstart)、[订阅使用适用范围](https://developers.openai.com/siwc/token-sharing-open-source)。
