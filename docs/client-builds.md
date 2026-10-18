# Web 构建

项目仅维护 React Web 界面，通过浏览器连接 Rust API。桌面与 Android 外壳、原生插件和安装包构建已移除。手机和平板通过响应式网页访问。

## 文件与来源

- 备份 JSON 和参考资料附件统一使用浏览器下载，保留中文文件名、UTF-8 文本与原始附件二进制内容。
- 文件选择、JSON 导入与 CSV 导入使用浏览器文件输入及现有校验。
- HTTP/HTTPS 来源链接在新标签页打开；无效或其他协议的来源仅显示文字。
- 复盘到期事项在页面内显示。

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

Web 资源、logo 和 API 使用同一部署前缀。生产登录页无需填写服务器地址，直接连接当前网站的 API。开发时 `npm run dev` 连接本机开发 API（`http://127.0.0.1:4217/api`，使用 `.runtime/dev` 中的 PostgreSQL），也可通过 `VITE_API_URL` 指定其他临时 API。

`npm run web:build` 生成 `client/dist` 并复制到 `outputs/web`。每次构建生成 `build-info.json`，记录版本、Git 提交、工作区状态、源码指纹、构建时间和资源路径。指纹覆盖 client、server、scripts、根目录 .gitattributes 与 package 文件，文档和忽略的配置、密钥、产物不参与。

`outputs/build-manifest.json` 仅记录当前 Web 产物的源码信息、大小与 SHA-256；目录校验按排序后的相对路径及内容计算。构建后源码变化会阻止产物记录，要求重新构建。

生产服务器执行根目录 `deploy.sh`，直接构建和服务 `client/dist`。部署、更新与验收见 [自建部署](self-hosting.md)。
