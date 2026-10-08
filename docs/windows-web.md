# Windows 与 Web

Web 通过自建 HTTPS 服务登录，Windows 使用同一服务地址和账号。完整部署见 [自建部署指南](self-hosting.md)。

```powershell
npm ci --prefix client
npm run windows:dev
npm run windows:build
```

Windows 要求 Rust MSVC、Microsoft C++ Build Tools 和 WebView2。构建出的 NSIS 安装包复制到 `outputs/`。桌面外壳不再启动本地服务或打包 sidecar，在登录页填写服务器 HTTPS 地址即可访问。

网页静态资源与 API 可以分别部署；Compose 中 Caddy 将 `/api/*` 反向代理到 API。远程多域名前端需配置 `VITE_API_URL` 与后端 `MARIO_ALLOWED_ORIGINS`。同源部署无需 CORS 配置。

`npm run web:build` 构建前端及独立 API 可执行文件；配置 PostgreSQL 和服务端密钥后，`npm run web:start` 可用单进程静态文件服务进行开发或内部测试。公网推荐使用 Compose 的 HTTPS 入口。
