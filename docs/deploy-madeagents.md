# 宿主机部署与 https://www.madeagents.ai/mario

服务器部署只使用项目根目录 **deploy.sh**。它直接构建并运行源码，不使用 Docker。IP、域名和所有日常参数集中在脚本顶部，每项都有中文注释。

## 首次部署

推荐 Ubuntu 22.04+ / Debian 12+、systemd、x86_64 或 arm64。域名 www 的 A 记录指向 `47.243.99.21`，安全组开放 80/443；数据库、Agent 和内部 API 端口不开放到公网。需要足够内存与磁盘完成 Rust 编译。

代码推送到远端后，在服务器克隆。普通用户建议放在自己的 home；root 登录时放在 `/srv/mario`，不要放进 `/root`：

```bash
git clone https://github.com/lokinko/mario.git /srv/mario
cd /srv/mario
sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
```

默认执行以下工作：

- Ubuntu/Debian 安装编译依赖和 PostgreSQL 17；缺少 Node 22+ 时安装项目专用 Node，并验证下载校验和；缺少 Rust 时为服务用户安装官方 rustup。
- 自动选择非 root 运行用户，首次生成数据库密码、主密钥与 Agent 令牌，持久保存在权限受限的 `.env`。
- 按 `/mario/` 构建 React 网页，编译 Rust 服务，自动初始化或复用 `.runtime/native/postgres`。
- 创建 `mario.service`，由 systemd 托管 PostgreSQL、API 和独立 Agent。断开 SSH 不停止服务，开机自动启动。
- 生成 `.runtime/native/nginx.conf`，供现有 HTTPS 站点接入。
- 从 Hugging Face 私有仓库下载 benchmark 切片并生成研究看板 `/mario/dashboard`，见下文[研究看板](#研究看板-mariodashboard)。

项目专用 Node 位于 `/opt/mario-tools/node`，不替换宿主机已有 Node。PostgreSQL 包由官方 PGDG 源安装；发行版可能同时创建自己的默认数据库服务，本项目使用独立的 55432 端口和数据目录。其他发行版请预装工具，并将脚本中的 `INSTALL_MISSING_TOOLS` 改成 `false`。

项目 PostgreSQL 只监听 `127.0.0.1` TCP，显式关闭 Unix socket，避免服务用户无权写入发行版默认的 `/var/run/postgresql`。启动和恢复临时启动均使用此设置，不影响系统数据库。

部署脚本以 root 执行系统安装，但运行服务使用普通用户。默认复用已有数据库所有者或 sudo 登录用户；直接 root 登录时创建 `mario`。部署会把当前项目目录归运行用户所有，不移动现有数据库，不更换密钥。

部署在安装工具、下载依赖和停止已有服务之前，自动准备服务用户的 home 和工具目录，修复历史部署留下的属主、写入和遍历权限，再以服务用户身份验证。系统依赖和 systemd 由 root 管理，Rust/uv 安装、配置生成、构建和 PostgreSQL/API/Agent 运行统一使用服务用户，避免两种身份竞争写入同一工具或运行目录。

安装来源：[Node 官方发行版](https://nodejs.org/dist/latest-v22.x/)、[PostgreSQL 官方源](https://www.postgresql.org/download/linux/ubuntu/)、[Rust 官方安装](https://rust-lang.org/tools/install/)、[uv 官方安装](https://docs.astral.sh/uv/getting-started/installation/)。

## 服务用户 home 与工具目录权限

若旧部署的 Rust 安装报 `could not create bin directory: '/home/mario/.cargo/bin': Permission denied`，更新代码后直接重新部署即可，无需先手工 `chown`：

```bash
sudo -u mario -H git -C /srv/mario pull --ff-only
sudo bash /srv/mario/deploy.sh
```

脚本自动创建已有账号缺失的 home；对默认专用 home `/home/<服务用户>`、`/var/lib/<服务用户>`，修复 root 遗留的归属。自定义的已有 home 必须属于服务用户，属于其他用户的目录保持原样。只调整 home 本身，不递归接管其余个人文件。

自动修复范围为 `.cargo`、`.rustup`、`.local/bin` 及 `.local` 父目录：工具目录内修复属主并补齐所有者读写权限和目录遍历权限，保留已有可执行位与 rustup 链接，不跟随内部链接修改外部目标；`.local` 下其他应用数据保持原样。缺失目录按服务用户创建。数据库、配置与密钥沿用原位置和运行账号，无需数据迁移。

后续仍用 `sudo bash deploy.sh` 部署；手工维护用户级工具也使用 `sudo -u mario -H`。目录被替换为文件、工具根目录是符号链接、home 属于其他用户等情况不自动接管，脚本会保留数据并报告路径。若修复后仍无法访问，可用 `namei -l /home/mario/.cargo/bin` 检查实际父目录与链接目标的权限。

## 配置只改一个地方

`deploy.sh` 顶部配置区中，通常只需 IP 与域名，也可以像上面那样首次通过参数提供。以后直接运行脚本，空的 IP/域名会复用已有 `.env`。默认路径 `/mario/` 对应你的当前需求。

可选配置说明包括网页前缀、API/Agent/PG 端口、已有数据库 URL、PG 工具路径、跨域来源、服务用户、systemd 名称和依赖安装开关。脚本中的参数会合并到 `.env`，已有密码与密钥保持原值。`.env` 是运行配置和秘密存储，不是第二份需要同时维护的部署参数表。模型 API 地址由用户在“模型与隐私”中自行配置，无需管理员添加白名单。

早期部署留下的 `.env.native` 会自动合并到 `.env` 并保留备份；如果新旧文件同时存在需先整理。保留并备份 `.env`，尤其 `MARIO_MASTER_KEY`；遗失主密钥后无法解密已有模型凭据。用户各自的模型 API Key 在网页登录后配置。

## 一次性接入已有 Nginx

若 www 会跳转到 `madeagents.ai`，需要让 `/mario` 留在 www。脚本无法仅凭 IP、域名判断现有站点文件、证书路径或面板布局，因此生成配置片段，**不自动覆盖原网站配置**。

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

本机 Codex 桥接随 `deploy.sh` 一起构建和部署，脚本自动检测桥接协议是否就绪。服务器不需要 Codex 登录；用户在网页配对后，在自己的电脑运行 `npm run codex:bridge`。详细步骤见 [本机 Codex 桥接](codex-bridge.md)。

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

升级后，已有账户首次访问 API 时会按数据库结构版本自动补齐表、列和数据版本触发器，保留原有数据；迁移在账户锁和事务内完成，失败会回滚。出现缺表或缺列错误时，同步最新代码并运行 `sudo bash deploy.sh`；`restart` 只重启已有二进制，不会编译修复。请求中的数据库错误可从服务日志和 `sudo tail -n 80 .runtime/native/postgres.log` 查看具体原因。

`prepare` 只准备 `.env` 和 Nginx 片段，适合恢复数据前使用，需已有 Node：

```bash
bash deploy.sh prepare --ip 47.243.99.21 --domain www.madeagents.ai
```

## 研究看板 /mario/dashboard

benchmark 策略得分看板与 Mario 一起部署，地址为 `https://www.madeagents.ai/mario/dashboard`，任何已登录账号都能查看；未登录只看到“前往 Mario 登录”。Nginx 不需要额外配置：`/mario/` 下的请求本来就会转给 API，`/dashboard`、`/dashboard.js` 和 `/api/benchmark/dashboard` 由 Rust 服务从 `MARIO_BENCH_DIR` 读取，只有最后一个数据接口需要登录。

`deploy.sh` 顶部 `BENCH_ENABLED=true` 启用持续看板，写入 `.env` 的 `MARIO_BENCH_ENABLED`；线上数据切片由 GitHub Actions 的 `BENCHMARK_SCORING_TAG` 统一指定（默认 `pool-scoring-v8`）。策略提交至 main 后自动评测、发布状态与成绩，无需再次部署网页。首次需设置仓库 Secret `HF_TOKEN` 并运行“Benchmark 持续评测”工作流。

构建阶段准备独立 Python 环境并下载私有标签仓库的 `dashboard/current.json`，本地生成页面与脚本。运行阶段由 systemd 主运行器启动只读同步器，每 30 秒检查远端更新，写入固定 `.runtime/benchmark/current/`；API 沿用账号认证提供数据，网页每 10 秒刷新状态。同步失败保留旧成绩，首次无快照则等待工作流完成；查看 `journalctl -u mario` 排查服务用户的 HF 读取权限。旧版本目录不删除，继续用于历史复现。详情见 [持续评测](benchmark.md#git-提交后持续评测)。

两个仓库都是私有的，服务器需要一个只读 token，首次部署后用服务用户保存一次：

```bash
sudo -u <服务用户> -H /srv/mario/.runtime/benchmark/.venv/bin/hf auth login
sudo bash deploy.sh
```

看板同步失败不阻止 Mario 部署：缺少 token、网络失败或尚无在线快照时，日志打印“研究看板未更新”、失败阶段（准备 Python 环境、同步在线评测快照）及原因。服务照常启动，继续提供上次同步的结果；从未生成时返回“尚未生成”，后台同步成功后即可访问。`bash deploy.sh check` 检查看板和数据接口的登录保护。`MARIO_BENCH_ENABLED=false` 时不启用同步。

更新策略只需推送 main 并查看 Actions 的评测结果。评分数据升级先发布新的不可变标签，再修改 `BENCHMARK_SCORING_TAG` 并手动运行持续评测；不再修改服务器标签或重新部署看板。

### Python 安装与权限故障

`uv pip install -e .` 曾因缺少打包配置而报 `Multiple top-level packages discovered in a flat-layout`，将 `client`、`server` 等目录误当 Python 包。当前 `pyproject.toml` 明确只打包 `benchmark` 及配置、策略资源，支持可编辑安装；安装核心包不安装可选的 Hugging Face 依赖，部署使用上面的 `publish` 依赖组。

若旧部署报 `failed to canonicalize path /srv/mario/.venv/bin/python: Permission denied`，常见原因是该解释器链接到 `/root/.local/share/uv/python/...`。`chown -R` 项目目录无法让服务用户穿过 `/root`。可用 `namei -l /srv/mario/.venv/bin/python` 核对每层目录与链接目标，不要放宽 `/root` 权限。

更新代码后直接重新执行 `sudo bash deploy.sh`：新部署在 `.runtime/benchmark/` 建立服务用户独立环境，不再访问旧 `.venv`，无需删除旧环境、数据库、配置或看板。若准备 Python 环境成功后出现私有仓库 401/403，再按上面的新路径登录 token 并重新部署。若仍在 Python 阶段失败，按详细错误检查项目父目录的遍历权限、磁盘空间及 Python 下载网络；HF 登录不能解决解释器权限问题。

## PostgreSQL 启动失败

`pg_ctl: could not start server` 只是摘要，具体错误在项目日志。先在服务器项目目录停止自动重启并查看：

```bash
sudo bash deploy.sh stop
sudo tail -n 80 .runtime/native/postgres.log
```

若出现 `could not create lock file "/var/run/postgresql/.s.PGSQL.55432.lock": Permission denied`，是系统 socket 目录权限问题；当前脚本只用 TCP，更新脚本后重新部署：

```bash
sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
```

运行器会把 PostgreSQL 日志末尾最多 16 KiB 输出到服务日志。若仍失败，按其中的 `FATAL`/`PANIC` 排查端口、权限、磁盘空间或数据版本；不要删除 `.runtime/native/postgres` 或 `.env` 来重试。

## 验收 Web

公网只读验收使用 `bash deploy.sh check`，检查入口跳转、API 模式、JS/CSS/图标、研究看板及未登录保护。也可在本机项目中运行：

```bash
node scripts/test-deployment.mjs --url https://www.madeagents.ai/mario
```

本机 API 检查仍用根路径：

```bash
curl -fsS http://127.0.0.1:4217/api/server
# hosted / postgresql / protocol 1
```

浏览器打开 **https://www.madeagents.ai/mario**，将规范为 `/mario/`。使用邮箱和密码直接注册，无需邀请码：

1. 账号 A 修改一条测试资料，刷新确认保留；另一设备登录 A，确认同步。
2. 注册 B，确认看不到 A 的数据；两个设备提交旧表单时应提示刷新/冲突。
3. 在“模型与隐私”配置自己的模型/API Key，测试连接并完成一次 AI 问答。
4. 执行脚本 `restart`，重新登录确认数据保留，最后生成一次加密备份。

301 去根域名检查旧 server 层跳转；502 检查 `status`/`logs`；资源 404 检查 `/mario/` 构建和代理尾斜杠；超时检查 DNS、安全组、防火墙。不要用 `curl -k` 绕过证书验证。

备份与频繁搬迁见 [数据迁移](data-migration.md)，账号、同步和密码重置见 [自建服务](self-hosting.md)。集成测试只用临时数据库，不连接线上数据库。
