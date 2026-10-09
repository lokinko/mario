# 宿主机部署与 https://www.madeagents.ai/mario

服务器部署只使用项目根目录 **deploy.sh**。它直接构建并运行源码，不使用 Docker。IP、域名和所有日常参数集中在脚本顶部，每项都有中文注释。

## 首次部署

推荐 Ubuntu 22.04+ / Debian 12+、systemd、x86_64 或 arm64。域名 www 的 A 记录指向 `47.243.99.21`，安全组开放 80/443；数据库、Agent 和内部 API 端口不开放到公网。需要足够内存与磁盘完成 Rust 编译。

把包含本次修改的代码提交并推送后，在服务器克隆。普通用户建议放在自己的 home；root 登录时放在 `/srv/mario`，不要放进 `/root`：

```bash
git clone https://github.com/lokinko/mario.git /srv/mario
cd /srv/mario
sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
```

默认执行以下工作：

- Ubuntu/Debian 安装编译依赖和 PostgreSQL 17；缺少 Node 22+ 时安装项目专用 Node，并验证下载校验和；缺少 Rust 时为服务用户安装官方 rustup。
- 自动选择非 root 运行用户，首次生成数据库密码、主密钥、邀请码与 Agent 令牌，持久保存在权限受限的 `.env`。
- 按 `/mario/` 构建 React 网页，编译 Rust 服务，自动初始化或复用 `.runtime/native/postgres`。
- 创建 `mario.service`，由 systemd 托管 PostgreSQL、API 和独立 Agent。断开 SSH 不停止服务，开机自动启动。
- 生成 `.runtime/native/nginx.conf`，供现有 HTTPS 站点接入。

项目专用 Node 位于 `/opt/mario-tools/node`，不替换宿主机已有 Node。PostgreSQL 包由官方 PGDG 源安装；发行版可能同时创建自己的默认数据库服务，本项目使用独立的 55432 端口和数据目录。其他发行版请预装工具，并将脚本中的 `INSTALL_MISSING_TOOLS` 改成 `false`。

部署脚本以 root 执行系统安装，但运行服务使用普通用户。默认复用已有数据库所有者或 sudo 登录用户；直接 root 登录时创建 `mario`。部署会把当前项目目录归运行用户所有，不移动现有数据库，不更换密钥。旧 Docker 数据不会自动转为宿主机数据；请用旧版本导出加密包后恢复。

安装来源：[Node 官方发行版](https://nodejs.org/dist/latest-v22.x/)、[PostgreSQL 官方源](https://www.postgresql.org/download/linux/ubuntu/)、[Rust 官方安装](https://rust-lang.org/tools/install/)。

## 配置只改一个地方

`deploy.sh` 顶部配置区中，通常只需 IP 与域名，也可以像上面那样首次通过参数提供。以后直接运行脚本，空的 IP/域名会复用已有 `.env`。默认路径 `/mario/` 对应你的当前需求。

可选配置说明包括网页前缀、API/Agent/PG 端口、已有数据库 URL、PG 工具路径、模型主机白名单、跨域来源、服务用户、systemd 名称和依赖安装开关。脚本中的参数会合并到 `.env`，已有密码与密钥保持原值。`.env` 是运行配置和秘密存储，不是第二份需要同时维护的部署参数表。

已有 `.env.native` 自动迁移并保留备份；如果新旧文件同时存在需先整理。保留并备份 `.env`，尤其 `MARIO_MASTER_KEY`；遗失主密钥后无法解密已有模型凭据。用户各自的模型 API Key 在网页登录后配置。

## 一次性接入已有 Nginx

2026-10-08 实际观测到 www 会跳转到 `madeagents.ai`；需要让 `/mario` 留在 www。脚本无法仅凭 IP、域名判断现有站点文件、证书路径或面板布局，因此生成配置片段，**不自动覆盖原网站配置**。

先备份现有配置。在 `www.madeagents.ai` 的 **443 HTTPS server** 中，保留原证书，加入生成片段，例如：

```nginx
include /srv/mario/.runtime/native/nginx.conf;
```

也可直接复制文件内容到该 server。默认生成的核心路由是：

```nginx
location = /mario { return 308 /mario/$is_args$args; }
location ^~ /mario/ {
    proxy_pass http://127.0.0.1:4217/;
    # 完整的请求头、超时、安全响应头见生成片段。
}
```

`proxy_pass` 末尾 `/` 用于剥离前缀，使公网 `/mario/api/server` 转成上游 `/api/server`。不要新增根路径 `/api` 的公网代理。见 [Nginx URI 替换规则](https://nginx.org/en/docs/http/ngx_http_proxy_module.html#proxy_pass)。

如果 www 的 `server` 层有 `return 301 https://madeagents.ai$request_uri;`，将它移入兜底的 `location /`，否则会在匹配 `/mario` 前直接重定向：

```nginx
location / {
    return 301 https://madeagents.ai$request_uri;
}
```

已有 `location /` 时修改原块，不要重复添加。根网站原来提供内容则保留其原配置。根域和 www 共用 server 时，如需完全保留根域行为，应拆出 www server 并复用覆盖 www 的证书。详见 [Nginx 执行顺序](https://nginx.org/en/docs/http/ngx_http_rewrite_module.html)。

80 的跳转应保留请求主机和路径，让 HTTP 进入相同 www 的 HTTPS。配置完成后：

```bash
sudo nginx -t && sudo systemctl reload nginx
bash deploy.sh check
```

面板/容器托管的 Nginx 在对应环境重载。若还没有 HTTPS 站点，需先创建覆盖 www 的 TLS 站点；IP 和域名本身不能替代证书。脚本不改变 DNS、云安全组或现有证书管理方式。

## 日常只用这个脚本

```bash
# 更新前备份；用仓库所有者账号拉取代码，然后重新部署、构建。
git pull --ff-only
sudo bash deploy.sh

sudo bash deploy.sh start
sudo bash deploy.sh stop
sudo bash deploy.sh restart
bash deploy.sh status
sudo bash deploy.sh logs
bash deploy.sh check
```

更改脚本顶部配置后执行 `deploy`，不能只 `restart`：重新部署才会写入新配置和重新构建网页。更改路径/端口后，生成的 Nginx 片段也会更新，需要重新检查并加载 Nginx。部署会先停止当前应用再构建，因此升级期间网站暂时不可用；构建失败保持服务停止，可在排查后重新部署。

`prepare` 只准备 `.env` 和 Nginx 片段，适合恢复数据前使用，需已有 Node：

```bash
bash deploy.sh prepare --ip 47.243.99.21 --domain www.madeagents.ai
```

## 验收 Web

公网只读验收使用 `bash deploy.sh check`，检查入口跳转、API 模式、JS/CSS/图标及未登录保护。也可在本机项目中运行：

```bash
node scripts/test-deployment.mjs --url https://www.madeagents.ai/mario
```

本机 API 检查仍用根路径：

```bash
curl -fsS http://127.0.0.1:4217/api/server
# hosted / postgresql / protocol 1
```

浏览器打开 **https://www.madeagents.ai/mario**，将规范为 `/mario/`。使用服务器 `.env` 中的 `MARIO_REGISTRATION_KEY` 注册：

1. 账号 A 修改一条测试资料，刷新确认保留；另一设备登录 A，确认同步。
2. 注册 B，确认看不到 A 的数据；两个设备提交旧表单时应提示刷新/冲突。
3. 在“模型与隐私”配置自己的模型/API Key，测试连接并完成一次 AI 问答。
4. 执行脚本 `restart`，重新登录确认数据保留，最后生成一次加密备份。

301 去根域名检查旧 server 层跳转；502 检查 `status`/`logs`；资源 404 检查 `/mario/` 构建和代理尾斜杠；超时检查 DNS、安全组、防火墙。不要用 `curl -k` 绕过证书验证。

备份与频繁搬迁见 [数据迁移](data-migration.md)，账号、同步和密码重置见 [自建服务](self-hosting.md)。集成测试只用临时数据库，不连接线上数据库。
