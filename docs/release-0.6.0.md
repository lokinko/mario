# mario 0.6.0

> 历史记录：其中的原生客户端与平台构建说明仅适用于当时版本。当前仅维护 Web，见 [Web 构建](client-builds.md)。

发布日期：2026-10-11。

## 更新

- 自建 Rust API、PostgreSQL 与独立 AI Agent，多账号和跨设备访问；统一部署、加密备份与迁移入口。
- 问答预设、完整报告和冻结来源引用；参考资料支持原文及附件。
- 简化“我的情况”录入，增加每日资产追踪、持仓成本和估值记录，优化导航及界面。
- Web、macOS、Windows、Android 共用页面与 API；原生文件保存与系统浏览器打开来源。
- 各端安装包同步升级至 0.6.0，附源码信息和 SHA-256 校验文件。

## 安装包

- `mario_0.6.0_aarch64.dmg`：macOS Apple Silicon。
- `mario_0.6.0_x64-setup.exe`：Windows x64，中文/英文安装向导，按用户安装；必要时下载 WebView2。
- `mario_0.6.0_android-aarch64-release.apk`：Android arm64，最低 Android 7.0，优化且关闭调试的 release 构建。
- `SHA256SUMS-0.6.0.txt`：三端安装包的 SHA-256。
- `build-manifest-0.6.0.json`：各端产物的源码提交、指纹与校验信息。

安装后填写自建服务器地址，例如 `https://www.madeagents.ai/mario`，再登录账号。客户端需要连接已部署的服务器。

## 签名与验证范围

macOS 使用临时签名，未进行 Apple 公证；Windows 安装器未使用发行商证书签名。Android release APK 沿用此前的调试密钥签名以保持升级兼容，不作为应用商店正式签名版本。

发布验证覆盖前端测试、构建、安装包格式、签名/完整性及源码一致性；不代表已完成 Windows 与 Android 真机交互验收。
