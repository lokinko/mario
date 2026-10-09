#!/usr/bin/env bash
# Mario 宿主机部署的唯一入口。Ubuntu/Debian + systemd；不需要 Docker。
# 首次：sudo bash deploy.sh --ip 47.243.99.21 --domain www.madeagents.ai
# 以后：sudo bash deploy.sh；管理：bash deploy.sh start|stop|restart|status|logs|check
# 首次自动安装缺少的工具、生成密钥、构建网页/API、创建 systemd 开机服务。
# 已有 HTTPS Nginx 需要一次性加入脚本生成的 location；不会覆盖原网站或证书。

# =================== 用户配置区 ===================
# 以下空值表示复用 .env 中的已有值；首次 IP/域名可通过命令参数传入。
SERVER_IP=""                    # 公网 IP，仅供 DNS 提示；服务始终监听回环，不直接绑定公网。
DOMAIN=""                       # 域名，例如 www.madeagents.ai，不含协议、端口、路径。
BASE_PATH="/mario/"              # 网页访问前缀，必须以 / 结尾；根站点部署改为 /。
API_PORT="4217"                 # 网页+API 的本机端口，Nginx 去掉前缀后转发到这里。
AGENT_PORT="4218"               # 独立 AI Agent 的本机端口，不开放给公网。
PG_PORT="55432"                 # 项目独立 PostgreSQL 的本机端口，不占用系统默认 5432。
PG_BIN=""                       # 可选 PostgreSQL bin 路径；自动查找 /usr/lib/postgresql/*/bin。
DATABASE_URL=""                 # 可选已有数据库 URL；空值复用已有配置，首次为空则使用项目本地 PG。
# URL 中若有 $ 等 Shell 字符，赋值使用单引号，密码需按 URL 规则编码。
MODEL_HOSTS=""                  # 可选模型 HTTPS 主机白名单，逗号分隔；默认 OpenAI/Anthropic。
ALLOWED_ORIGINS=""              # 可选跨域网页 Origin，逗号分隔；当前同源网页无需填写。
RUN_USER=""                     # 服务用户；默认复用数据库所有者或 sudo 登录用户，root 登录则创建 mario。
SERVICE_NAME="mario"            # systemd 服务名；一台服务器部署多份时需使用不同名称和端口。
INSTALL_MISSING_TOOLS="true"    # true 自动安装缺失工具；false 由你提前安装 Node 22+/Rust/PG。
# 不要手写数据库密码或密钥：首次自动生成并保存 .env，重启不会更换。
# 用户自己的模型/API Key 在网页登录后的“模型与隐私”里配置。
# .env 是自动生成的运行配置和秘密存储；迁移时必须一并备份，不提交 Git。
# 数据目录继续使用 .runtime/native/postgres，兼容此前宿主机部署和加密备份。
# =================== 配置区结束 ===================

set -euo pipefail
umask 077
PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$PROJECT_DIR/deploy.sh"
ORIGINAL_ARGS=("$@")
ACTION="deploy"
if [[ $# -gt 0 && "$1" != -* ]]; then ACTION="$1"; shift; fi
while [[ $# -gt 0 ]]; do
    case "$1" in
        --ip|--domain)
            [[ $# -ge 2 && "$2" != --* ]] || { echo "$1 缺少值" >&2; exit 1; }
            if [[ "$1" == --ip ]]; then SERVER_IP="$2"; else DOMAIN="$2"; fi
            shift 2 ;;
        --help|-h) ACTION="help"; shift ;;
        *) echo "未知参数：$1；使用 bash deploy.sh --help" >&2; exit 1 ;;
    esac
done
case "$ACTION" in
    help)
        cat <<'HELP'
用法：sudo bash deploy.sh [deploy|prepare|start|stop|restart|status|logs|check] [--ip 公网IP] [--domain 域名]
  deploy   默认：安装缺失依赖、准备配置、构建、启用/启动后台服务（已有服务先停止）。
  prepare  仅生成/更新 .env 和 Nginx 片段，不启动服务、不安装依赖；需已有 Node。
  start / stop / restart / status / logs  管理本项目 systemd 服务。
  check    使用现有 .env 对公网域名和网页路径执行只读验收。
首次可传 IP、域名；其他配置见脚本顶部中文注释。以后无需重复传参。
Ubuntu/Debian 自动安装依赖，其他 Linux 需预先安装 Node 22+、Rust、PostgreSQL。
Linux 部署请将仓库放在普通用户 home 或 /srv/mario，不放在 /root 内。
准备完成后按提示将 Nginx 片段加入现有 HTTPS server，再运行 check。
HELP
        exit 0 ;;
    deploy|prepare|start|stop|restart|status|logs|check) ;;
    *) echo "未知操作：$ACTION" >&2; exit 1 ;;
esac
[[ "$SERVICE_NAME" =~ ^[a-zA-Z0-9_-]+$ ]] || { echo 'SERVICE_NAME 格式无效' >&2; exit 1; }
[[ "$INSTALL_MISSING_TOOLS" == true || "$INSTALL_MISSING_TOOLS" == false ]] || { echo 'INSTALL_MISSING_TOOLS 必须为 true 或 false' >&2; exit 1; }
[[ "$PROJECT_DIR" != *$'\n'* && "$PROJECT_DIR" != *'%'* && "$PROJECT_DIR" != *'"'* && "$PROJECT_DIR" != *'\'* && "$PROJECT_DIR" != *' '* ]] || { echo '部署目录不能包含空格、换行、%、反斜杠或引号' >&2; exit 1; }
cd "$PROJECT_DIR"
# 使用项目专属 Node，不更换宿主机其他服务使用的 Node。
export PATH="/opt/mario-tools/node/bin:$PATH"
if [[ "$ACTION" == deploy || "$ACTION" == start || "$ACTION" == stop || "$ACTION" == restart || "$ACTION" == logs ]]; then
    [[ "$(uname -s)" == Linux ]] || { echo 'systemd 部署仅支持 Linux；本地测试请使用开发命令' >&2; exit 1; }
    if [[ "$EUID" -ne 0 ]]; then exec sudo bash "$SCRIPT" "${ORIGINAL_ARGS[@]}"; fi
    command -v systemctl >/dev/null || { echo '需要启用 systemd 的 Linux 主机' >&2; exit 1; }
fi
if [[ "$ACTION" == deploy && -e .runtime/restore.lock ]]; then echo '恢复操作尚未完成，请先检查 .runtime/restore.lock' >&2; exit 1; fi
case "$ACTION" in
    start|stop|restart) exec systemctl "$ACTION" "$SERVICE_NAME.service" ;;
    status) exec systemctl status "$SERVICE_NAME.service" --no-pager ;;
    logs) exec journalctl -u "$SERVICE_NAME.service" -n 100 -f ;;
esac

node_ready() { command -v node >/dev/null && node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 22 ? 0 : 1)' && command -v npm >/dev/null; }
install_tools() {
    if [[ "$INSTALL_MISSING_TOOLS" == true ]]; then
        # Ubuntu/Debian 软件源只用于宿主机依赖；不会使用 Docker。
        command -v apt-get >/dev/null || { echo '非 Ubuntu/Debian 请设置 INSTALL_MISSING_TOOLS=false 并预装工具' >&2; exit 1; }
        if ! command -v curl >/dev/null || ! command -v gcc >/dev/null || ! command -v make >/dev/null || ! command -v xz >/dev/null || ! command -v pkg-config >/dev/null || ! pkg-config --exists openssl; then
            apt-get update
            DEBIAN_FRONTEND=noninteractive apt-get install -y curl ca-certificates xz-utils build-essential pkg-config libssl-dev
        fi
        if ! node_ready; then
            local arch manifest archive
            case "$(uname -m)" in x86_64) arch=x64 ;; aarch64|arm64) arch=arm64 ;; *) echo 'Node 自动安装支持 x86_64/arm64' >&2; exit 1 ;; esac
            local work
            work="$(mktemp -d)"
            curl -fsS https://nodejs.org/dist/latest-v22.x/SHASUMS256.txt -o "$work/SHASUMS256.txt"
            manifest="$(awk -v suffix="-linux-$arch.tar.xz" 'substr($2, length($2)-length(suffix)+1)==suffix {print; exit}' "$work/SHASUMS256.txt")"
            archive="${manifest##* }"
            [[ "$archive" =~ ^node-v22\.[0-9]+\.[0-9]+-linux-(x64|arm64)\.tar\.xz$ ]] || { echo 'Node 下载清单无效' >&2; exit 1; }
            curl -fsS "https://nodejs.org/dist/latest-v22.x/$archive" -o "$work/$archive"
            (cd "$work"; printf '%s\n' "$manifest" | sha256sum --check --status)
            install -d -m 755 /opt/mario-tools/node
            tar -xJf "$work/$archive" -C /opt/mario-tools/node --strip-components=1
            chmod 755 /opt/mario-tools /opt/mario-tools/node
            # work 来自 mktemp，成功后只删除这两个下载文件，空目录由 rmdir 清理。
            rm -- "$work/$archive" "$work/SHASUMS256.txt"
            rmdir -- "$work"
        fi
        if [[ -z "$DATABASE_URL" && -z "$PG_BIN" && ! -x /usr/lib/postgresql/17/bin/pg_ctl ]]; then
            # PostgreSQL 官方 PGDG 源，锁定 17；原有配置中指定的 PG_BIN 优先。
            . /etc/os-release
            [[ "$ID" == ubuntu || "$ID" == debian ]] || { echo 'PG 自动安装仅支持 Ubuntu/Debian' >&2; exit 1; }
            [[ "${VERSION_CODENAME:-}" =~ ^[a-z]+$ ]] || { echo '无法确定系统版本代号' >&2; exit 1; }
            install -d -m 755 /usr/share/postgresql-common/pgdg
            curl -fsS https://www.postgresql.org/media/keys/ACCC4CF8.asc -o /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
            chmod 644 /usr/share/postgresql-common/pgdg/apt.postgresql.org.asc
            printf 'deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] https://apt.postgresql.org/pub/repos/apt %s-pgdg main\n' "$VERSION_CODENAME" > /etc/apt/sources.list.d/mario-pgdg.list
            chmod 644 /etc/apt/sources.list.d/mario-pgdg.list
            apt-get update
            DEBIAN_FRONTEND=noninteractive apt-get install -y postgresql-17 postgresql-client-17
        fi
    fi
    node_ready || { echo '需要 Node.js 22+ 和 npm' >&2; exit 1; }
}

if [[ "$ACTION" == deploy ]]; then
    [[ "$PROJECT_DIR" != /root && "$PROJECT_DIR" != /root/* ]] || { echo '请将仓库克隆到 /srv/mario 或普通用户 home，PostgreSQL 不能作为 root 运行' >&2; exit 1; }
    if [[ -z "$RUN_USER" ]]; then
        if [[ -f .runtime/native/postgres/PG_VERSION ]]; then RUN_USER="$(stat -c %U .runtime/native/postgres/PG_VERSION)";
        elif [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != root ]]; then RUN_USER="$SUDO_USER";
        else RUN_USER="mario"; fi
    fi
    [[ "$RUN_USER" =~ ^[a-z_][a-z0-9_-]*\$?$ && "$RUN_USER" != root ]] || { echo '服务用户必须是有效的非 root 账号' >&2; exit 1; }
    if ! id "$RUN_USER" >/dev/null 2>&1; then useradd --system --create-home --shell /bin/bash "$RUN_USER"; fi
    # 只接管当前项目；已有数据库须仍归运行用户所有，避免误接管别的 PostgreSQL。
    if [[ -f .runtime/native/postgres/PG_VERSION && "$(stat -c %U .runtime/native/postgres/PG_VERSION)" != "$RUN_USER" ]]; then echo '数据库所有者与 RUN_USER 不一致，请保留原运行用户' >&2; exit 1; fi
    [[ -f package.json && -f server/Cargo.toml && -f scripts/host-runtime.mjs ]] || { echo '不是完整的 Mario 源码目录' >&2; exit 1; }
    install_tools
    chown -R "$RUN_USER:$(id -gn "$RUN_USER")" -- "$PROJECT_DIR"
    RUN_HOME="$(getent passwd "$RUN_USER" | cut -d: -f6)"
    export PATH="$RUN_HOME/.cargo/bin:$PATH"
    as_user() { runuser -u "$RUN_USER" -- env -u CARGO_HOME -u RUSTUP_HOME PATH="$PATH" "$@"; }
    if ! as_user cargo --version >/dev/null 2>&1; then
        [[ "$INSTALL_MISSING_TOOLS" == true ]] || { echo '请先给运行用户安装 Rust/rustup' >&2; exit 1; }
        # Rust 官方安装器作为普通服务用户运行，版本由 rust-toolchain.toml 固定。
        installer="$(mktemp)"
        curl -fsS https://sh.rustup.rs -o "$installer"
        chmod 644 "$installer"
        as_user sh "$installer" -y --profile minimal --default-toolchain none
        rm -- "$installer"
    fi
    NODE="$(command -v node)"
    # 重复部署停止已有服务后构建，防止网页构建目录被正在运行的 API 使用。
    if systemctl cat "$SERVICE_NAME.service" >/dev/null 2>&1; then systemctl stop "$SERVICE_NAME.service"; fi
else
    command -v node >/dev/null || { echo 'prepare/check 需要已有 Node；首次服务器部署请用 sudo bash deploy.sh --ip ... --domain ...' >&2; exit 1; }
    NODE="$(command -v node)"
fi

if [[ "$ACTION" == check ]]; then exec "$NODE" scripts/test-deployment.mjs; fi
export MARIO_SETUP_IP="$SERVER_IP" MARIO_SETUP_DOMAIN="$DOMAIN" MARIO_SETUP_BASE_PATH="$BASE_PATH"
export MARIO_SETUP_API_PORT="$API_PORT" MARIO_SETUP_AGENT_PORT="$AGENT_PORT" MARIO_SETUP_PG_PORT="$PG_PORT"
export MARIO_SETUP_PG_BIN="$PG_BIN" MARIO_SETUP_DATABASE_URL="$DATABASE_URL"
export MARIO_SETUP_MODEL_HOSTS="$MODEL_HOSTS" MARIO_SETUP_ORIGINS="$ALLOWED_ORIGINS"
if [[ "$ACTION" == prepare ]]; then exec "$NODE" scripts/host-runtime.mjs --prepare; fi
as_user "$NODE" scripts/host-runtime.mjs --prepare
as_user "$NODE" scripts/host-runtime.mjs --build

# systemd 仅向主运行器发送 SIGTERM，由运行器依次关闭 API/Agent/PG。
# TimeoutStopSec 给 pg_ctl 安全停止留出时间；意外退出后自动重启整个服务。
unit="$(mktemp)"
cat > "$unit" <<UNIT
[Unit]
Description=Mario Web API PostgreSQL and Agent
After=network-online.target
Wants=network-online.target
[Service]
Type=simple
User=$RUN_USER
WorkingDirectory=$PROJECT_DIR
ExecStart=$NODE $PROJECT_DIR/scripts/host-runtime.mjs --skip-build
Restart=on-failure
RestartSec=5
KillMode=mixed
TimeoutStopSec=90
UMask=0077
NoNewPrivileges=true
[Install]
WantedBy=multi-user.target
UNIT
install -m 644 "$unit" "/etc/systemd/system/$SERVICE_NAME.service"
rm -- "$unit"
systemctl daemon-reload
systemctl enable --now "$SERVICE_NAME.service"
for attempt in $(seq 1 120); do
    main_pid="$(systemctl show "$SERVICE_NAME.service" --property=MainPID --value)"
    launcher_pid="$(cat .runtime/native/launcher.lock 2>/dev/null || true)"
    if [[ "$main_pid" != 0 && "$main_pid" == "$launcher_pid" ]] && "$NODE" --input-type=module -e 'const r=await fetch(process.argv[1],{signal:AbortSignal.timeout(2000)}); const b=await r.json(); process.exit(r.ok && b.mode==="hosted" && b.database==="postgresql" ? 0 : 1);' "http://127.0.0.1:$API_PORT/api/server" >/dev/null 2>&1; then break; fi
    if [[ "$attempt" == 120 ]]; then journalctl -u "$SERVICE_NAME.service" -n 30 --no-pager; echo '服务未就绪' >&2; exit 1; fi
    sleep 1
done
echo "宿主机服务已就绪，关闭 SSH 后继续运行，重启主机自动启动。"
echo "将 $PROJECT_DIR/.runtime/native/nginx.conf 加入域名的 HTTPS server，运行 nginx -t 后重载。"
echo '若 www 在 server 层跳到根域名，请把该 return 移到兜底 location /，让 /mario 留在 www。'
echo '详细步骤：docs/deploy-madeagents.md；公网验收：bash deploy.sh check；日志：sudo bash deploy.sh logs'
