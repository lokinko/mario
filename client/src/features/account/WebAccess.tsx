import { useState, type ReactNode } from "react";
import { isWebApp, setWebToken, webToken } from "../../lib/webSession";
import { requestJson } from "../../lib/transport";

export function WebAccess({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(() => Boolean(webToken()));
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  if (!isWebApp()) return children;
  if (connected)
    return (
      <>
        <button
          className="web-disconnect"
          onClick={() => {
            setWebToken();
            setToken("");
            setConnected(false);
          }}
        >
          断开 Web 连接
        </button>
        {children}
      </>
    );
  return (
    <div className="web-access">
      <form
        className="panel"
        onSubmit={async (event) => {
          event.preventDefault();
          if (busy) return;
          const local = ["localhost", "127.0.0.1", "[::1]"].includes(
            location.hostname,
          );
          if (!local && location.protocol !== "https:") {
            setError("远程访问请使用 HTTPS 地址。");
            return;
          }
          setBusy(true);
          setError("");
          try {
            await requestJson(
              `${import.meta.env.VITE_API_URL ?? "/api"}/health`,
              {
                headers: { Authorization: `Bearer ${token.trim()}` },
              },
            );
            setWebToken(token.trim());
            setConnected(true);
          } catch {
            setError("连接失败，请核对访问密钥及服务地址。");
          } finally {
            setBusy(false);
          }
        }}
      >
        <img src="/mario-mark.svg" width="48" height="48" alt="" />
        <h1>连接 mario</h1>
        <label>
          访问密钥
          <input
            type="password"
            autoComplete="off"
            autoFocus
            required
            value={token}
            onChange={(event) => setToken(event.target.value)}
          />
        </label>
        <p>使用服务器的访问密钥。资料保存在该服务器。</p>
        {error && (
          <div className="error-box" role="alert">
            {error}
          </div>
        )}
        <button className="primary" disabled={busy || !token.trim()}>
          {busy ? "连接中…" : "连接"}
        </button>
      </form>
    </div>
  );
}
