import { useEffect, useRef, useState } from "react";
import { Copy, ExternalLink, ShieldCheck } from "lucide-react";
import { Badge } from "../../components/Badge";
import {
  getCodexBridgeStatus,
  pairCodexBridge,
  enableCodexBridge,
  revokeCodexBridge,
  startCodexBridgeOAuth,
  getModelConfig,
  type CodexBridgeStatus,
} from "../../api/settings";
import type { ModelConfig } from "../../types/ai";

const CHATGPT_USAGE_URL = "https://chatgpt.com/settings/usage";

export function CodexBridge({
  onUpdate,
  disabled,
  onStatus,
}: {
  onUpdate: (model: ModelConfig) => void;
  disabled: boolean;
  onStatus?: (status: CodexBridgeStatus | null) => void;
}) {
  const [status, setStatus] = useState<CodexBridgeStatus | null>(null);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [statusError, setStatusError] = useState(false);
  const alive = useRef(true);
  const generation = useRef(0);
  const statusCallback = useRef(onStatus);
  statusCallback.current = onStatus;
  useEffect(() => {
    statusCallback.current?.(statusError ? null : status);
  }, [status, statusError]);
  useEffect(() => {
    alive.current = true;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      const current = generation.current;
      try {
        const next = await getCodexBridgeStatus();
        if (!disposed && alive.current && current === generation.current) {
          setStatus(next);
          setStatusError(false);
          if (next.online || !next.paired) setCode("");
        }
      } catch {
        if (!disposed && alive.current && current === generation.current) {
          setStatus(null);
          setStatusError(true);
        }
      }
      if (!disposed && alive.current) timer = setTimeout(refresh, 3000);
    };
    void refresh();
    return () => {
      alive.current = false;
      disposed = true;
      clearTimeout(timer);
    };
  }, []);

  async function copyText(text: string, successText: string) {
    try {
      await navigator.clipboard.writeText(text);
      if (alive.current) setMessage(successText);
    } catch {
      if (alive.current) setMessage("复制失败，请手动选中复制。");
    }
  }

  async function startOAuth(newAccount: boolean) {
    setBusy(true);
    setMessage("");
    try {
      const { url } = await startCodexBridgeOAuth(newAccount);
      if (!alive.current) return;
      const parsed = new URL(url);
      if (
        parsed.origin !== "https://auth.openai.com" ||
        parsed.pathname !== "/api/accounts/authorize"
      ) {
        throw new Error("返回的 ChatGPT OAuth 授权地址无效");
      }
      const opened = window.open(url, "_blank", "noopener,noreferrer");
      if (!opened) {
        window.location.assign(url);
      }
      setMessage(
        "已打开 ChatGPT 官方 OAuth 登录页，授权完成后将自动回跳本机 127.0.0.1 并刷新模型列表。",
      );
    } catch (error) {
      if (alive.current) setMessage(String(error));
    } finally {
      if (alive.current) setBusy(false);
    }
  }

  async function act(action: "pair" | "enable" | "revoke") {
    setBusy(true);
    setMessage("");
    generation.current++;
    try {
      if (action === "pair") {
        const next = await pairCodexBridge();
        if (!alive.current) return;
        setCode(next.pairingCode);
        setStatus({
          paired: true,
          online: false,
          model: null,
          mode: null,
          accountLabel: null,
          authRequired: false,
        });
        setMessage("配对码 10 分钟内有效。重新配对会断开原连接。");
      } else if (action === "enable") {
        const next = await enableCodexBridge();
        if (!alive.current) return;
        setCode("");
        onUpdate(next);
        setMessage("已启用本机模型，可点击下方测试已保存连接。");
      } else {
        await revokeCodexBridge();
        const next = await getModelConfig();
        if (!alive.current) return;
        onUpdate(next);
        setCode("");
        setStatus({
          paired: false,
          online: false,
          model: null,
          mode: null,
          accountLabel: null,
          authRequired: false,
        });
        setMessage("已撤销配对，本机进程无法继续接收请求。");
      }
    } catch (error) {
      if (alive.current) setMessage(String(error));
    } finally {
      generation.current++;
      if (alive.current) setBusy(false);
    }
  }

  const modeBadge = statusError
    ? { tone: "warning" as const, label: "状态不可用" }
    : status === null
      ? { tone: "neutral" as const, label: "检查中" }
      : status.online
        ? status.authRequired
          ? { tone: "warning" as const, label: "待登录 ChatGPT" }
          : status.mode === "chatgpt"
            ? { tone: "success" as const, label: "ChatGPT 官方授权" }
            : { tone: "success" as const, label: "本机在线" }
        : status.paired
          ? { tone: "warning" as const, label: "等待连接" }
          : { tone: "neutral" as const, label: "未配对" };

  return (
    <div className="codex-bridge-panel">
      <div className="codex-bridge-header">
        <strong>本机 ChatGPT 官方授权</strong>
        <Badge tone={modeBadge.tone}>{modeBadge.label}</Badge>
      </div>

      <div className="codex-bridge-status-card">
        <div className="codex-bridge-status-main">
          <p role="status" className="codex-bridge-status-line">
            {statusError
              ? "无法读取连接状态，请确认服务已部署新版并检查网络。"
              : status === null
                ? "正在检查连接状态…"
                : status.online
                  ? status.authRequired
                    ? "本机桥接已连接 · 请点击下方按钮跳转 ChatGPT 完成 OAuth 授权"
                    : `本机在线 · 默认模型 ${status.model}`
                  : status.paired
                    ? "等待本机连接"
                    : "尚未配对或配对已过期"}
          </p>
          {status?.online && status.mode === "chatgpt" && (
            <div className="codex-bridge-account-meta">
              {!status.authRequired && (
                <span>
                  <ShieldCheck size={14} aria-hidden="true" />
                  {status.accountLabel
                    ? `已授权账号：${status.accountLabel}`
                    : "已通过本机 Sign in with ChatGPT 官方授权连接"}
                </span>
              )}
              <div className="codex-bridge-oauth-actions">
                <button
                  type="button"
                  className="secondary siwc-button"
                  data-oai-siwc-button=""
                  disabled={busy || disabled}
                  onClick={() => void startOAuth(false)}
                >
                  <span className="siwc-mark" aria-hidden="true" />
                  <span>
                    {status.authRequired
                      ? "跳转 ChatGPT OAuth 登录"
                      : "重新登录 ChatGPT"}
                  </span>
                </button>
                {!status.authRequired && (
                  <button
                    type="button"
                    className="secondary"
                    disabled={busy || disabled}
                    onClick={() => void startOAuth(true)}
                  >
                    切换新 ChatGPT 账号
                  </button>
                )}
                <a
                  className="codex-bridge-usage-link"
                  href={CHATGPT_USAGE_URL}
                  target="_blank"
                  rel="noopener noreferrer"
                >
                  管理 ChatGPT 用量与授权
                  <ExternalLink size={13} aria-hidden="true" />
                </a>
              </div>
            </div>
          )}
        </div>
      </div>

      <ol className="codex-bridge-steps">
        <li>点击“生成配对码”。</li>
        <li>
          在电脑的 Mario 目录运行：
          <ul className="codex-bridge-commands">
            <li>
              <div className="codex-bridge-command-row">
                <span>
                  ChatGPT 官方登录：<code>npm run chatgpt:bridge</code>
                </span>
                <button
                  type="button"
                  className="ghost codex-bridge-copy"
                  disabled={busy || disabled}
                  aria-label="复制 ChatGPT 官方登录启动命令"
                  onClick={() =>
                    void copyText(
                      "npm run chatgpt:bridge",
                      "已复制 npm run chatgpt:bridge",
                    )
                  }
                >
                  <Copy size={13} aria-hidden="true" />
                  复制命令
                </button>
              </div>
              <small>
                粘贴配对码连接后，直接在上方点击“跳转 ChatGPT OAuth
                登录”完成授权。
              </small>
            </li>
          </ul>
          连接其他站点可附加 <code>-- https://你的站点/mario/</code>
          ，按提示粘贴配对码。
        </li>
        <li>完成登录并显示在线后点击“启用本机模型”。</li>
      </ol>

      {code && (
        <label className="codex-bridge-pairing">
          配对码（仅本次显示）
          <div className="codex-bridge-pairing-row">
            <input
              aria-label="配对码"
              type="password"
              readOnly
              value={code}
              autoComplete="off"
            />
            <button
              type="button"
              className="secondary"
              disabled={busy || disabled}
              onClick={() =>
                void copyText(code, "配对码已复制，请粘贴到本机桥接窗口。")
              }
            >
              复制配对码
            </button>
          </div>
        </label>
      )}

      <div className="form-actions">
        <button
          type="button"
          className="secondary"
          disabled={busy || disabled}
          onClick={() => void act("pair")}
        >
          {status?.paired ? "重新配对" : "生成配对码"}
        </button>
        <button
          type="button"
          className="secondary"
          disabled={
            busy || disabled || !status?.online || Boolean(status?.authRequired)
          }
          onClick={() => void act("enable")}
        >
          启用本机模型
        </button>
        {status?.paired && (
          <button
            type="button"
            className="danger-text"
            disabled={busy || disabled}
            onClick={() => void act("revoke")}
          >
            撤销配对
          </button>
        )}
      </div>

      {message && <p role="status">{message}</p>}
      <small className="codex-bridge-note">
        授权仅在本机（127.0.0.1）完成，凭据由本机系统加密保管且不上传服务器；配对有效期
        12 小时。查看或撤销订阅额度请前往{" "}
        <a href={CHATGPT_USAGE_URL} target="_blank" rel="noopener noreferrer">
          ChatGPT 设置 → Usage
        </a>
        。
      </small>
    </div>
  );
}
