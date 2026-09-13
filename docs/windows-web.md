# Windows 与 Web

## Windows 桌面版

支持 Windows 10/11 x64。开发环境安装 Node.js 22、Rust MSVC 工具链、Microsoft C++ Build Tools（桌面 C++ 工作负载）和 WebView2，参见 [Tauri 环境要求](https://v2.tauri.app/start/prerequisites/)。

```powershell
npm run install:all
npm run windows:dev
npm run windows:build
```

构建脚本先生成带目标平台后缀的服务端 sidecar，再打包 NSIS 安装程序到 `outputs/*-setup.exe`。安装器支持中文/英文，安装到当前用户，自动检查 WebView2。密钥使用 Windows Credential Manager，数据库仍在系统的应用数据目录中。安装包目前未做代码签名。

仓库的 `Windows build` 工作流在 Windows runner 上运行前端、Rust 和真实接口测试，随后构建并上传安装器。它不自动发布 Release。打包方式参见 [Tauri Windows 安装器](https://v2.tauri.app/distribute/windows-installer/)。

Windows 模型入口支持 API Key；若使用 Codex 集成，需要 `codex.exe` 在 PATH 中，登录属于运行 mario 的 Windows 用户。

## Web 访问

Web 使用同一套页面、API、SQLite 和每日资产规则，可在浏览器里使用。它是一个人的投资档案服务；持有访问密钥的人能访问该档案。云账户登录用于加密同步，不是 Web 多租户隔离。

在已具备 Rust 和 Node.js 的 macOS 或 Windows 上：

```sh
npm run web:build
npm run web:start
```

浏览器打开 `http://127.0.0.1:4217`，输入启动时提示的密钥文件中的内容。默认 Web 数据目录为 `outputs/web-data`，首次生成密钥，重启继续使用；与桌面默认档案分开。密钥只保存在当前浏览器标签页的会话存储中，点击“断开 Web 连接”清除。关闭标签页会结束这份浏览器会话。

| 环境变量 | 用途 |
| --- | --- |
| `MARIO_DATA_DIR` | 服务端数据库及默认 Web 访问密钥的位置 |
| `MARIO_AUTH_TOKEN` | 自行设置 32–256 字符的随机访问密钥；不要提交到仓库 |
| `MARIO_PORT` | 启动脚本端口，默认 4217 |
| `MARIO_HOST` | 监听 IP，默认 127.0.0.1 |
| `MARIO_WEB_DIR` | 直接运行服务端时指定编译后的静态目录；启动脚本自动设为 client/dist |

需要手机或另一台电脑访问时，在服务器前配置 HTTPS 反向代理，将网页和 `/api` 一起代理到回环端口。浏览器拒绝向非本机的 HTTP 地址提交访问密钥。应用不会自动开放公网端口、修改防火墙或部署到外部主机。

静态页面可以公开加载，全部实际 API 都校验 Bearer 访问密钥；数据库、密钥文件不放在静态目录。不要把 `MARIO_WEB_DIR` 指向数据目录或项目根目录。Web 模式拒绝以未认证开发参数启动。

模型、行情与同步凭据属于运行服务器的系统用户，保存在服务器的原生凭据存储中。Web 的“本机”是服务器，不是访问者的浏览器；Codex 集成也使用服务器上的 Codex 登录。当前打包入口面向 macOS/Windows 主机，尚未提供无桌面 Linux 的持久凭据后端。系统通知仍属于原生客户端。

## 验证

`npm test` 除既有测试外运行 `scripts/test-web.mjs`，验证静态资源、API 未授权拒绝、无效密钥拒绝、私有文件不可访问及重启持久化。前端测试覆盖 Web 连接/断开和抽屉关闭后的焦点恢复。Windows 原生构建结果由仓库工作流提供。
