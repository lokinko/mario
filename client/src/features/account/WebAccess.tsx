import { useEffect, useState, type ReactNode } from "react";
import { setWebToken, webToken } from "../../lib/webSession";
import { requestJson } from "../../lib/transport";
import {
  acceptRevision,
  isLocalDev,
  isNative,
  normalizeServerUrl,
  saveServerUrl,
  serviceUrl,
} from "../../lib/service";
import { blockStaleWrites } from "../../lib/syncEvents";

export function WebAccess({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(() => Boolean(webToken()));
  const [server, setServer] = useState(() =>
    serviceUrl() === "/api" ? "" : serviceUrl(),
  );
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [invite, setInvite] = useState("");
  const [register, setRegister] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const clear = () => {
    setWebToken();
    acceptRevision();
    blockStaleWrites(false);
    setPassword("");
    setConnected(false);
  };
  useEffect(() => {
    window.addEventListener("mario:session-expired", clear);
    return () => window.removeEventListener("mario:session-expired", clear);
  }, []);
  if (isLocalDev()) return children;
  if (connected)
    return (
      <>
        <button
          className="web-disconnect"
          onClick={async () => {
            try {
              await requestJson(`${serviceUrl()}/auth/logout`, {
                method: "POST",
                headers: { Authorization: `Bearer ${webToken()}` },
              });
              clear();
            } catch {
              setError("退出失败，请联网后重试以撤销服务端会话。");
            }
          }}
        >
          退出账号
        </button>
        {error && <p role="alert">{error}</p>}
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
          setBusy(true);
          setError("");
          try {
            const base = server.trim() ? normalizeServerUrl(server) : "/api";
            if (
              base === "/api" &&
              !["localhost", "127.0.0.1", "[::1]"].includes(
                location.hostname,
              ) &&
              location.protocol !== "https:"
            )
              throw new Error("远程访问请使用 HTTPS");
            const result = await requestJson<{ token: string }>(
              `${base}/auth/${register ? "register" : "login"}`,
              {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                  email,
                  password,
                  ...(register ? { registrationKey: invite } : {}),
                }),
              },
            );
            if (server.trim()) saveServerUrl(server);
            acceptRevision();
            blockStaleWrites(false);
            setWebToken(result.token);
            setPassword("");
            setInvite("");
            setConnected(true);
          } catch (error) {
            setError(error instanceof Error ? error.message : String(error));
          } finally {
            setBusy(false);
          }
        }}
      >
        <img src="/mario-mark.svg" width="48" height="48" alt="" />
        <h1>{register ? "创建 mario 账号" : "登录 mario"}</h1>
        {(isNative() || server) && (
          <label>
            服务器地址
            <input
              type="url"
              required
              value={server}
              placeholder="https://mario.example.com"
              onChange={(e) => setServer(e.target.value)}
            />
          </label>
        )}
        <label>
          邮箱
          <input
            type="email"
            required
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </label>
        <label>
          密码
          <input
            type="password"
            required
            minLength={12}
            maxLength={256}
            autoComplete={register ? "new-password" : "current-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        {register && (
          <label>
            邀请码
            <input
              type="password"
              required
              autoComplete="off"
              value={invite}
              onChange={(e) => setInvite(e.target.value)}
            />
          </label>
        )}
        <p>资料存储在你的服务器，同一账号可在网页和客户端访问。</p>
        {error && (
          <div className="error-box" role="alert">
            {error}
          </div>
        )}
        <button className="primary" disabled={busy}>
          {busy ? "连接中…" : register ? "创建账号" : "登录"}
        </button>
        <button
          type="button"
          onClick={() => {
            setRegister(!register);
            setError("");
          }}
        >
          {register ? "已有账号，去登录" : "使用邀请码注册"}
        </button>
      </form>
    </div>
  );
}
