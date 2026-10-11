# 客户端同步构建

Web、macOS、Windows 和 Android 共用 `client/src` 中的页面与业务 API，包括问答预设、完整报告、冻结来源、纯文本决策与复盘、参考资料附件和分析看板。原生外壳不再维护一份独立业务实现。

## 文件与来源

- 网页使用浏览器下载；macOS、Windows、Android 使用系统保存对话框，取消选择不会写入文件。
- 参考资料附件、问答引用附件、账号 JSON 备份和组合流水 CSV 模板使用同一保存入口。中文文件名、UTF-8 文本、CSV BOM 与原始附件二进制内容保持完整。
- 原生端的网页来源交给系统浏览器打开；网页仍打开新标签页。无效或非 HTTP/HTTPS 来源只显示文字。
- 附件选择、JSON 导入与 CSV 导入保留 WebView 文件选择器及现有校验。Android 的文件选择由 WebView 提供系统文档选择器。
- 原生文件权限仅启用保存对话框和写文件命令，桌面路径由用户选择后授予；Android 使用系统授予的 `content://` URI。没有授权任意文件路径或原生文件读取命令。

实现依据为 Tauri 官方的 [Dialog](https://v2.tauri.app/plugin/dialog/)、[File System](https://v2.tauri.app/plugin/file-system/) 和 [Opener](https://v2.tauri.app/plugin/opener/)；依赖固定为与仓库 Tauri 2.8 / Rust 1.86 兼容的版本。

## 构建入口

```bash
# macOS：outputs/mario.app 与 DMG
npm run desktop:build
# Android：outputs/ 下的 aarch64 调试 APK
npm run android:build
# Android：优化后的 aarch64 release APK
npm run android:release
# 网页：outputs/web，可通过 VITE_BASE_PATH 设置部署前缀
VITE_BASE_PATH=/mario/ npm run web:build
# 本机依次构建 macOS、Android、Web
npm run build:all
```

Windows 在配置了 MSVC 与 WebView2 的机器上运行 `npm run windows:build`，或使用仓库的 `Windows build` 工作流。工作流上传安装器及其构建清单。macOS 不能直接替代 Windows 的编译和交互验收。

Tauri 使用 `build:native`，资源路径固定为 `/`，不受网页部署前缀影响。原生登录页填写完整服务器地址，例如 `https://www.madeagents.ai/mario`。

每次前端构建产生 `build-info.json`，记录 Git 提交、是否存在未提交改动、源码内容指纹、构建时间、目标和资源路径。指纹覆盖 client、server、scripts、工作流与根目录 package 文件；文档和忽略的本地配置、密钥、产物不参与指纹。

成功的平台构建会更新 `outputs/build-manifest.json`：每个产物分别记录自己的源码信息、大小和 SHA-256；目录产物按排序后的相对路径及内容计算 SHA-256。旧安装包保留自己的旧提交并标为 `stale`，不会因另一个平台重新打包而被标为最新。如果前端构建后源码发生变化，记录脚本会阻止写入错误的版本信息，要求重新构建。

网页或服务器部署不会自动更新已安装客户端的内置页面；生成新安装包后仍需安装。macOS 当前为临时签名。Android 支持 debug 和 release 构建；默认沿用本机调试密钥签名以兼容已有安装，可通过 `ANDROID_KEYSTORE`、`ANDROID_KEY_ALIAS`、`ANDROID_KEYSTORE_PASSWORD` 和 `ANDROID_KEY_PASSWORD` 配置正式签名。release 构建会压缩代码并关闭调试，使用调试密钥签名仍不等于正式签名。
