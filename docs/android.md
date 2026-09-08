# Android 调试构建

## 运行方式

Android APK 复用桌面版的 React UI、类型化 HTTP 客户端、领域规则、SQLite 模型与 AI 工作流。平台差异只发生在本地服务的装载方式：

```text
Tauri Android WebView
        │ random loopback port + one-time bearer token
        ▼
embedded mario-server Rust library
        ├── app sandbox / mario.db
        └── Android Keystore / encrypted SharedPreferences
```

桌面端仍启动独立的 `mario-server` sidecar；Android 不下载、不启动外部可执行文件，而是在 Tauri 应用进程内运行同一个 Axum Router。这样 API、校验、同步和投资方法论保持单一实现。

## 构建环境

构建需要 JDK 17、Android SDK Platform、Platform Tools、Build Tools、Side-by-side NDK，以及所选 ABI 对应的 Rust Android target。仓库脚本尊重已有的 `JAVA_HOME`、`ANDROID_HOME` 与 `NDK_HOME`；未设置时使用 Homebrew 默认位置：

- `JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home`
- `ANDROID_HOME=/opt/homebrew/share/android-commandlinetools`
- `NDK_HOME` 自动选择 `ANDROID_HOME/ndk` 下最新版本

首次初始化：

```bash
npm run android:init
```

`client/src-tauri/gen/android` 是可再生的 Tauri/Gradle 工程，因此不进入 Git；初始化后，构建脚本会应用 Android 本地回环通信所需的 Manifest 设置。

## 生成与安装调试 APK

```bash
npm run android:build
adb install -r outputs/mario_0.4.1_android-aarch64-debug.apk
```

需要让调试 APK 首次启动即绑定某个 Supabase 项目时，在构建进程中传入公开客户端配置：

```bash
VITE_SUPABASE_URL=https://<project-ref>.supabase.co \
VITE_SUPABASE_PUBLISHABLE_KEY=<sb_publishable_...> \
npm run android:build
```

已有的本机云配置优先，不会被构建值覆盖。这里只能使用 Publishable Key；Secret/`service_role` Key 不得进入 APK。

默认只构建现代 Android 真机使用的 ARM64 (`aarch64`) APK，并在打包时移除原生调试符号以控制体积。需要其他真机或模拟器 ABI 时，可以显式指定，例如：

```bash
MARIO_ANDROID_TARGETS="aarch64 armv7 i686 x86_64" npm run android:build
```

对应 Rust targets 需要事先安装。调试 APK 使用 Android 自动生成的 debug 签名，只适合开发测试；发布到应用商店前必须单独建立发布密钥、release 构建和签名保管流程。

## 安全边界

- SQLite 位于 Android 应用专属沙盒，不申请共享存储权限。
- 模型密钥、行情密钥、账户令牌与恢复密钥通过彼此独立的 Android Keystore 条目加密保存。
- 本地服务仅绑定 `127.0.0.1`，每次启动使用新的 256 位令牌；所有 API（包括健康检查）都要求令牌。
- WebView 访问 loopback 需要 Android cleartext 开关；前端 CSP 仍只允许自身、loopback 与 HTTPS，CORS 只允许受信任的 Tauri/开发来源。
- APK 不在后台常驻服务，进程退出后本地 HTTP 服务随之结束。
