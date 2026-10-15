import { useEffect, useRef, useState } from "react";
import {
  getCodexBridgeStatus,
  pairCodexBridge,
  enableCodexBridge,
  revokeCodexBridge,
  getModelConfig,
  type CodexBridgeStatus,
} from "../../api/settings";
import type { ModelConfig } from "../../types";

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
  async function act(action: "pair" | "enable" | "revoke") {
    setBusy(true);
    setMessage("");
    generation.current++;
    try {
      if (action === "pair") {
        const next = await pairCodexBridge();
        if (!alive.current) return;
        setCode(next.pairingCode);
        setStatus({ paired: true, online: false, model: null });
        setMessage("配对码 10 分钟内有效。重新配对会断开原连接。");
      } else if (action === "enable") {
        const next = await enableCodexBridge();
        if (!alive.current) return;
        setCode("");
        onUpdate(next);
        setMessage("已启用本机 Codex，可点击下方测试已保存连接。");
      } else {
        await revokeCodexBridge();
        const next = await getModelConfig();
        if (!alive.current) return;
        onUpdate(next);
        setCode("");
        setStatus({ paired: false, online: false, model: null });
        setMessage("已撤销配对，本机进程无法继续接收请求。");
      }
    } catch (error) {
      if (alive.current) setMessage(String(error));
    } finally {
      generation.current++;
      if (alive.current) setBusy(false);
    }
  }
  return (
    <div className="codex-bridge-panel">
      <strong>连接你电脑上的 Codex</strong>
      <p>
        在本机登录 Codex
        后，生成配对码并启动桥接。电脑需要保持联网，桥接进程需要持续运行。
      </p>
      <ol>
        <li>点击“生成配对码”。</li>
        <li>
          在电脑的 Mario 项目目录运行 <code>npm run codex:bridge</code>
          ，按提示粘贴配对码。
        </li>
        <li>显示在线后点击“启用本机 Codex”。</li>
      </ol>
      <p role="status">
        {statusError
          ? "无法读取连接状态，请确认服务已部署新版并检查网络。"
          : status === null
            ? "正在检查连接状态…"
            : status.online
              ? `本机在线 · 默认模型 ${status.model}`
              : status.paired
                ? "等待本机连接"
                : "尚未配对或配对已过期"}
      </p>
      {code && (
        <label>
          配对码（仅本次显示）
          <input
            aria-label="配对码"
            type="password"
            readOnly
            value={code}
            autoComplete="off"
          />
          <button
            className="secondary"
            disabled={busy || disabled}
            onClick={() => {
              void Promise.resolve()
                .then(() => navigator.clipboard.writeText(code))
                .then(() => {
                  if (alive.current)
                    setMessage("配对码已复制，请粘贴到本机桥接窗口。");
                })
                .catch(() => {
                  if (alive.current)
                    setMessage("复制失败，请选中配对码手动复制。");
                });
            }}
          >
            复制配对码
          </button>
        </label>
      )}
      <div className="form-actions">
        <button
          className="secondary"
          disabled={busy || disabled}
          onClick={() => void act("pair")}
        >
          {status?.paired ? "重新配对" : "生成配对码"}
        </button>
        <button
          className="secondary"
          disabled={busy || disabled || !status?.online}
          onClick={() => void act("enable")}
        >
          启用本机 Codex
        </button>
        {status?.paired && (
          <button
            className="danger-text"
            disabled={busy || disabled}
            onClick={() => void act("revoke")}
          >
            撤销配对
          </button>
        )}
      </div>
      {message && <p role="status">{message}</p>}
      <small>
        配对最长有效 12 小时；服务器重启后需重新配对。只转发你在 Mario
        中提交的模型任务，ChatGPT 登录凭据保留在本机。
      </small>
    </div>
  );
}
