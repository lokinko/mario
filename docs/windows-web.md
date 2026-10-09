# Windows 客户端与 Web

网页通过自建 HTTPS 服务登录，Windows 客户端使用同一服务地址和账号。宿主机部署只有根目录 `deploy.sh`，见 [服务器部署](deploy-madeagents.md)。

```powershell
npm ci --prefix client
npm run windows:dev
npm run windows:build
```

Windows 客户端构建要求 Rust MSVC、Microsoft C++ Build Tools 和 WebView2，安装包位于 `outputs/`。桌面外壳不启动服务或 sidecar，在登录页填写 `https://www.madeagents.ai/mario` 即可连接服务器；旧安装包需重新构建后才能接受子路径。

宿主机 API 同时提供构建后的静态网页，已有 Nginx 提供 HTTPS 并剥离部署前缀。单独调试前端可设置 `VITE_API_URL`，跨域时后端填写 `MARIO_ALLOWED_ORIGINS`；同源网页无需 CORS 配置。

Windows 本机领域开发仍使用 `npm run dev`；多用户服务的启动集成测试使用内部运行器和隔离 PostgreSQL。生产服务器统一在 Linux 上执行 `deploy.sh`，不再提供独立 Web 或 Docker 启动脚本。
