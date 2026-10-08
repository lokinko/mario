# Android 客户端

Android 现在是轻量 Tauri 客户端，和网页/桌面连接相同自建 API。它不再内嵌 Rust 后端或 SQLite，不读取服务器上的个人 Codex 登录；登录后配置自己的模型 API Key。

按已有脚本安装 JDK 17、Android SDK/NDK 与 Rust Android target：

```bash
npm run android:init
npm run android:build
```

启动应用，输入自建服务器的 HTTPS 地址并登录。API、PostgreSQL 和 Agent 配置见 [自建部署](self-hosting.md)。离线无法读取或提交新的服务端数据。Android 安装包需要在具备 Android 构建工具链的机器上验证。
