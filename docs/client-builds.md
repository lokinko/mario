# Web 构建

项目只有 React Web 界面，通过浏览器连接 Rust API；手机和平板通过响应式网页访问。

## 文件与来源

- 备份 JSON 和参考资料附件统一使用浏览器下载，保留中文文件名、UTF-8 文本与原始附件二进制内容。
- JSON 备份导入与参考资料附件选取使用浏览器文件输入，由服务端校验。
- HTTP/HTTPS 来源链接在新标签页打开；无效或其他协议的来源仅显示文字。
- 复盘与手工规则在各自分类中维护，当前交互见[复盘规范](review-and-rules.md)。

## 构建与部署

```bash
npm ci --prefix client
# 默认从 / 访问
npm run web:build
# Linux/macOS shell 中设置子路径
VITE_BASE_PATH=/mario/ npm run web:build
```

PowerShell 设置子路径：

```powershell
$env:VITE_BASE_PATH = '/mario/'
npm run web:build
```

Web 资源、logo 和 API 使用同一部署前缀。默认连接当前网站的 API；开发时 `npm run dev` 默认连接本机开发 API（`http://127.0.0.1:4217/api`，使用 `.runtime/dev` 中的 PostgreSQL），`VITE_API_URL` 可替换构建时默认值。登录页和“设置”的服务器连接可修改服务器地址：只接受 http(s) 地址，先请求 `/health` 确认可达后按部署前缀保存到本机 `localStorage` 并重新加载页面，可一键恢复默认；不同标签页同步切换。目标服务器须在 `MARIO_ALLOWED_ORIGINS` 放行当前网页来源；生产 Nginx 的 CSP 为 `connect-src 'self'`，线上页面只能改到同源路径，连接其他服务器需使用开发前端或调整部署。

`npm run web:build` 生成 `client/dist` 并复制到 `outputs/web`。Vite 的构建插件生成 `build-info.json`，记录版本、Git 提交、工作区状态、源码指纹、构建时间和资源路径。指纹按 Git 列出的已跟踪文件与未忽略的新文件计算，范围为 `client/`、`server/`、`scripts/`、根目录 `.gitattributes` 与 package 文件；文档和忽略的配置、密钥、产物不参与指纹。`sourceDirty` 单独来自整个工作区状态，因此仅文档修改也可能将它标记为脏工作区。

`outputs/build-manifest.json` 仅记录当前 Web 产物的源码信息、大小与 SHA-256；目录校验按排序后的相对路径及内容计算。构建后源码变化会阻止产物记录，要求重新构建。

生产服务器执行根目录 `deploy.sh`，由运行器调用 `npm run build --prefix client`，直接构建和服务 `client/dist`；这条路径会生成 `build-info.json`，但不复制 `outputs/web` 或更新 `outputs/build-manifest.json`。源码提交、构建来源与运行中服务是否已更新须分别核对，不能仅凭 Git 推送或旧分发 manifest 判定已经部署。部署、更新与验收见 [自建部署](self-hosting.md)。

构建实现见 [`build-web.mjs`](../scripts/build-web.mjs)、[`client-source.mjs`](../scripts/client-source.mjs) 和 [`vite.config.ts`](../client/vite.config.ts)。当前 `npm test` 包含前端构建、格式检查与组件测试，并通过 [`test-web.mjs`](../scripts/test-web.mjs) 验证静态资源、认证和重启持久化。完整门禁将后端构建与 HTTP 验收绑定至同一 Cargo target，忽略继承的测试二进制覆盖；独立 Web 验收仍可明确指定 `MARIO_TEST_SERVER_BIN`，未指定则按 `CARGO_TARGET_DIR` 定位。路径规则和子进程回归见[模块边界](module-boundaries.md#开发流程)。完整门禁不运行 `web:build` 的分发复制，也不构成手机或浏览器视觉验收。
