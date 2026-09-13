import type { SyncState } from "../../lib/autoSync";
import { CLOUD_DATA_UPDATED } from "../../lib/syncEvents";
import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  Cloud,
  Copy,
  Download,
  KeyRound,
  LoaderCircle,
  LockKeyhole,
  LogOut,
  Save,
  ShieldCheck,
  Upload,
  UserRound,
} from "lucide-react";
import {
  exportCloudRecoveryKey,
  getCloudConfig,
  getCloudStatus,
  importCloudRecoveryKey,
  pullCloudSync,
  pushCloudSync,
  resendCloudConfirmation,
  requestCloudPasswordReset,
  resetCloudPassword,
  saveCloudConfig,
  saveAutoSyncSettings,
  signInCloud,
  signOutCloud,
  signUpCloud,
  verifyCloudPasswordReset,
} from "../../api";
import type { CloudStatus } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { resolveBundledCloudConfig } from "../../lib/cloudConfig";

const bundledCloudConfig = resolveBundledCloudConfig(import.meta.env);

export function CloudSync({
  flash,
  onRestore,
  autoState,
  onRetryAuto,
}: {
  flash: (message: string) => void;
  onRestore: () => Promise<void>;
  autoState?: SyncState;
  onRetryAuto?: () => void;
}) {
  const [status, setStatus] = useState<CloudStatus | null>(null);
  const [url, setUrl] = useState("");
  const [publishableKey, setPublishableKey] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [recoveryKey, setRecoveryKey] = useState("");
  const [revealedKey, setRevealedKey] = useState("");
  const [authMode, setAuthMode] = useState<
    "login" | "signup" | "recover" | "verify" | "reset"
  >("login");
  const [authError, setAuthError] = useState("");
  const [authMessage, setAuthMessage] = useState("");
  const [proof, setProof] = useState("");
  const [recoveryId, setRecoveryId] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState("");
  const [initializing, setInitializing] = useState(true);
  const initialization = useRef(0);

  const refresh = async () => {
    const nextStatus = await getCloudStatus();
    setStatus(nextStatus);
    if (nextStatus.email) setEmail(nextStatus.email);
  };

  const initialize = async () => {
    const request = ++initialization.current;
    setInitializing(true);
    setError("");
    try {
      const storedConfig = await getCloudConfig();
      if (request !== initialization.current) return;
      const config = storedConfig ?? bundledCloudConfig;
      if (config) {
        setUrl(config.url);
        setPublishableKey(config.publishableKey);
      }
      const nextStatus =
        !storedConfig && bundledCloudConfig
          ? await saveCloudConfig(bundledCloudConfig)
          : await getCloudStatus();
      if (request !== initialization.current) return;
      setStatus(nextStatus);
      if (nextStatus.email) setEmail(nextStatus.email);
    } catch (nextError) {
      if (request === initialization.current)
        setError(
          nextError instanceof Error ? nextError.message : String(nextError),
        );
    } finally {
      if (request === initialization.current) setInitializing(false);
    }
  };

  useEffect(() => {
    void initialize();
    return () => {
      initialization.current += 1;
    };
  }, []);

  useEffect(() => {
    const update = () => {
      void refresh().catch(() => undefined);
    };
    window.addEventListener(CLOUD_DATA_UPDATED, update);
    return () => window.removeEventListener(CLOUD_DATA_UPDATED, update);
  }, []);
  useEffect(() => {
    if (autoState?.phase === "synced") void refresh().catch(() => undefined);
  }, [autoState?.phase]);

  const execute = async (name: string, action: () => Promise<string>) => {
    setBusy(name);
    setError("");
    setResult("");
    try {
      const message = await action();
      setResult(message);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setBusy("");
    }
  };

  const persistConfig = () =>
    execute("config", async () => {
      const next = await saveCloudConfig({ url, publishableKey });
      setStatus(next);
      flash("云端服务配置已保存");
      return "服务地址只用于账户认证和加密数据包同步。";
    });

  const authExecute = async (name: string, action: () => Promise<void>) => {
    setBusy(name);
    setAuthError("");
    setAuthMessage("");
    try {
      await action();
    } catch (nextError) {
      setAuthError(
        nextError instanceof Error ? nextError.message : String(nextError),
      );
    } finally {
      setBusy("");
    }
  };

  const changeAuthMode = (mode: typeof authMode) => {
    setAuthMode(mode);
    setAuthError("");
    setAuthMessage("");
    setPassword("");
    setProof("");
    setRecoveryId("");
    setNewPassword("");
    setConfirmPassword("");
  };

  const authenticate = (mode: "signup" | "login") =>
    authExecute(mode, async () => {
      const response =
        mode === "signup"
          ? await signUpCloud(email, password)
          : await signInCloud(email, password);
      setPassword("");
      await refresh();
      flash(response.message);
      setAuthMessage(response.message);
      if (!response.signedIn) setAuthMode("login");
    });

  const resendConfirmation = () =>
    authExecute("resend", async () => {
      const response = await resendCloudConfirmation(email);
      await refresh();
      flash(response.message);
      setAuthMessage(response.message);
    });

  const requestReset = () =>
    authExecute("recover", async () => {
      const response = await requestCloudPasswordReset(email.trim());
      setEmail(email.trim());
      setProof("");
      setRecoveryId("");
      setAuthMode("verify");
      setAuthMessage(response.message);
    });

  const verifyReset = () =>
    authExecute("verify", async () => {
      const response = await verifyCloudPasswordReset(email, proof);
      setRecoveryId(response.recoveryId);
      setProof("");
      setAuthMode("reset");
      setAuthMessage("邮箱验证成功，请在 10 分钟内设置新密码。");
    });

  const finishReset = () =>
    authExecute("reset", async () => {
      if (newPassword !== confirmPassword)
        throw new Error("两次输入的新密码不一致");
      const response = await resetCloudPassword(recoveryId, newPassword);
      setNewPassword("");
      setConfirmPassword("");
      setRecoveryId("");
      setPassword("");
      setAuthMode("login");
      setAuthMessage(response.message);
    });

  const logout = () =>
    execute("logout", async () => {
      setStatus(await signOutCloud());
      setPassword("");
      return "已退出账户；本地投资数据和恢复密钥仍保留在此设备。";
    });

  const push = () =>
    execute("push", async () => {
      const response = await pushCloudSync();
      await refresh();
      await onRestore();
      flash(`已同步云端版本 ${response.revision}`);
      return `${response.message}（${response.recordCount} 条记录）`;
    });

  const pull = async (replace = false) => {
    if (
      replace &&
      !window.confirm(
        "拉取会以云端快照替换本机的投资域数据。模型配置、模型密钥和账户配置不会改变。确认继续？",
      )
    )
      return;
    await execute("pull", async () => {
      const response = await pullCloudSync(replace);
      await refresh();
      await onRestore();
      flash(`已合并云端版本 ${response.revision}`);
      return `${response.message}（${response.recordCount} 条记录）`;
    });
  };

  const revealRecoveryKey = async () => {
    if (
      !window.confirm(
        "恢复密钥可以解密你的云端投资数据。仅在私密环境中显示，并请离线保管。继续？",
      )
    )
      return;
    await execute("reveal", async () => {
      const response = await exportCloudRecoveryKey();
      setRevealedKey(response.recoveryKey);
      return response.warning;
    });
  };

  const copyRecoveryKey = async () => {
    await navigator.clipboard.writeText(revealedKey);
    flash("恢复密钥已复制，请离线保管");
  };

  const importRecoveryKey = async () => {
    if (
      status?.hasRecoveryKey &&
      !window.confirm(
        "本机已有恢复密钥。替换后，原密钥对应的云端密文可能无法在本机解密。确认替换？",
      )
    )
      return;
    await execute("import", async () => {
      await importCloudRecoveryKey(
        recoveryKey,
        Boolean(status?.hasRecoveryKey),
      );
      setRecoveryKey("");
      await refresh();
      flash("恢复密钥已保存到系统钥匙串");
      return "现在可以拉取同一账户的云端加密快照。";
    });
  };

  if (initializing)
    return (
      <div className="center-screen" role="status">
        <LoaderCircle className="spin" />
        <span>正在读取账户与同步状态…</span>
      </div>
    );
  if (!status)
    return (
      <div className="center-screen service-error">
        <AlertTriangle size={28} />
        <strong>暂时无法读取账户状态</strong>
        <p role="alert">{error}</p>
        <p>本地投资功能仍可使用，你的数据没有被修改。</p>
        <button className="primary" onClick={() => void initialize()}>
          重新尝试
        </button>
      </div>
    );

  return (
    <div className="page narrow cloud-page">
      <PageHeader
        title="账户与同步"
        description="投资数据加密后同步。"
        action={
          <div className={`status-dot ${status.signedIn ? "connected" : ""}`}>
            {status.signedIn ? status.email : "未登录"}
          </div>
        }
      />

      <details className="panel form-panel" open={!status.configured}>
        <summary>自定义云端服务</summary>

        <div className="form-grid single-column">
          <label>
            <span>Project URL</span>
            <input
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              placeholder="https://your-project.supabase.co"
            />
          </label>
          <label>
            <span>Publishable / anon Key</span>
            <div className="secure-input">
              <KeyRound size={16} />
              <input
                value={publishableKey}
                onChange={(event) => setPublishableKey(event.target.value)}
                placeholder="公开客户端 Key，不要填写 service_role Key"
              />
            </div>
          </label>
        </div>
        <div className="privacy-note">
          <ShieldCheck size={18} />
          <div>
            <strong>不要使用 service_role Key</strong>
            <p>
              客户端只需要 Publishable/anon Key；数据库由 RLS
              和带版本比较的写入函数限制。
            </p>
          </div>
        </div>
        <div className="form-actions">
          <p>更换服务地址会退出当前账户并清除同步基线，不删除本地数据。</p>
          <button
            className="primary"
            onClick={persistConfig}
            disabled={busy !== "" || !url || !publishableKey}
          >
            <Save size={16} />
            保存服务配置
          </button>
        </div>
      </details>

      <section className="panel form-panel">
        <div className="panel-title">
          <div>
            <h2>
              {status.signedIn
                ? "当前账户"
                : {
                    login: "登录账户",
                    signup: "创建账户",
                    recover: "忘记密码",
                    verify: "验证重置邮件",
                    reset: "设置新密码",
                  }[authMode]}
            </h2>
          </div>
        </div>
        {status.signedIn ? (
          <div className="account-card">
            <div>
              <strong>{status.email}</strong>
              <span>登录令牌仅保存在系统钥匙串</span>
            </div>
            <button
              className="danger-text"
              onClick={logout}
              disabled={busy !== ""}
            >
              <LogOut size={14} />
              退出账户
            </button>
          </div>
        ) : (
          <>
            <div className="form-grid single-column">
              <label>
                <span>邮箱</span>
                <input
                  type="email"
                  autoComplete="email"
                  disabled={
                    busy !== "" || authMode === "verify" || authMode === "reset"
                  }
                  value={email}
                  onChange={(event) => {
                    setEmail(event.target.value);
                    setAuthError("");
                  }}
                  placeholder="name@example.com"
                />
              </label>
              {(authMode === "login" || authMode === "signup") && (
                <label>
                  <span>密码</span>
                  <div className="secure-input">
                    <LockKeyhole size={16} />
                    <input
                      type="password"
                      autoComplete={
                        authMode === "login"
                          ? "current-password"
                          : "new-password"
                      }
                      value={password}
                      onChange={(event) => setPassword(event.target.value)}
                      placeholder={
                        authMode === "signup" ? "至少 8 个字符" : "输入登录密码"
                      }
                    />
                  </div>
                </label>
              )}
              {authMode === "verify" && (
                <label>
                  <span>邮件验证码或重置链接</span>
                  <input
                    type="password"
                    autoComplete="off"
                    value={proof}
                    onChange={(event) => setProof(event.target.value)}
                    placeholder="输入验证码，或粘贴邮件按钮的完整链接"
                  />
                  <small>
                    如果邮件只有按钮，请右键或长按“Reset
                    Password”并复制链接地址，回到这里粘贴，无需打开链接。链接已打开或过期时，可重新发送。
                  </small>
                </label>
              )}
              {authMode === "reset" && (
                <>
                  <label>
                    <span>新密码</span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={newPassword}
                      onChange={(event) => setNewPassword(event.target.value)}
                      placeholder="至少 8 个字符"
                    />
                  </label>
                  <label>
                    <span>确认新密码</span>
                    <input
                      type="password"
                      autoComplete="new-password"
                      value={confirmPassword}
                      onChange={(event) =>
                        setConfirmPassword(event.target.value)
                      }
                    />
                  </label>
                  {confirmPassword && newPassword !== confirmPassword && (
                    <p role="alert">两次输入的新密码不一致</p>
                  )}
                </>
              )}
            </div>
            {authError && (
              <div className="error-box" role="alert">
                <AlertTriangle size={18} />
                {authError}
              </div>
            )}
            {authMessage && (
              <div className="connection-success" role="status">
                <Check size={16} />
                <span>{authMessage}</span>
              </div>
            )}
            {authMode === "login" && status.emailConfirmationPending && (
              <p>请先点击注册确认邮件中的链接，再返回登录。</p>
            )}
            {!status.configured ? (
              <p id="auth-requirements" role="alert">
                尚未连接账户服务，请先在上方保存云端服务配置后登录。
              </p>
            ) : (authMode === "login" || authMode === "signup") &&
              (!email.trim() ||
                !password ||
                (authMode === "signup" && password.length < 8)) ? (
              <p id="auth-requirements">
                {authMode === "signup"
                  ? "请输入邮箱和至少 8 个字符的密码。"
                  : "请输入邮箱和密码后登录。"}
              </p>
            ) : null}
            <div className="form-actions">
              <p>
                {authMode === "login"
                  ? "还没有账户？选择去注册。忘记密码可以通过邮箱重置。"
                  : authMode === "signup"
                    ? "注册后请检查邮箱中的确认邮件。"
                    : "重置登录密码后，云端数据仍需原同步恢复密钥解密。"}
              </p>
              <div className="key-actions">
                {authMode !== "login" && (
                  <button
                    className="secondary"
                    onClick={() => changeAuthMode("login")}
                    disabled={busy !== ""}
                  >
                    返回登录
                  </button>
                )}
                {authMode === "login" && (
                  <>
                    <button
                      className="secondary"
                      onClick={() => changeAuthMode("signup")}
                      disabled={busy !== ""}
                    >
                      去注册
                    </button>
                    <button
                      className="secondary"
                      onClick={() => changeAuthMode("recover")}
                      disabled={busy !== ""}
                    >
                      忘记密码
                    </button>
                    <button
                      className="secondary"
                      onClick={resendConfirmation}
                      disabled={
                        busy !== "" || !status.configured || !email.trim()
                      }
                    >
                      重发确认邮件
                    </button>
                  </>
                )}
                {(authMode === "login" || authMode === "signup") && (
                  <button
                    className="primary"
                    onClick={() => authenticate(authMode)}
                    aria-describedby="auth-requirements"
                    disabled={
                      busy !== "" ||
                      !status.configured ||
                      !email.trim() ||
                      (authMode === "signup" ? password.length < 8 : !password)
                    }
                  >
                    {busy === authMode
                      ? "正在处理…"
                      : authMode === "login"
                        ? "登录"
                        : "创建账户"}
                  </button>
                )}
                {(authMode === "recover" || authMode === "verify") && (
                  <button
                    className="secondary"
                    onClick={requestReset}
                    disabled={
                      busy !== "" || !status.configured || !email.trim()
                    }
                  >
                    {authMode === "verify"
                      ? "重新发送重置邮件"
                      : "发送重置邮件"}
                  </button>
                )}
                {authMode === "verify" && (
                  <button
                    className="primary"
                    onClick={verifyReset}
                    disabled={busy !== "" || !proof.trim()}
                  >
                    验证邮件
                  </button>
                )}
                {authMode === "reset" && (
                  <>
                    <button
                      className="secondary"
                      onClick={() => changeAuthMode("recover")}
                      disabled={busy !== ""}
                    >
                      重新验证邮箱
                    </button>
                    <button
                      className="primary"
                      onClick={finishReset}
                      disabled={
                        busy !== "" ||
                        newPassword.length < 8 ||
                        newPassword !== confirmPassword
                      }
                    >
                      保存新密码
                    </button>
                  </>
                )}
              </div>
            </div>
          </>
        )}
      </section>

      <section className="panel form-panel">
        <div className="panel-title">
          <div>
            <h2>自动同步</h2>
          </div>
          <div
            className={`status-dot ${!status.localChangedSinceSync && status.baseRevision > 0 ? "connected" : ""}`}
          >
            {status.baseRevision > 0
              ? `云端版本 ${status.baseRevision}`
              : "尚未同步"}
          </div>
        </div>
        <label className="auto-external-option">
          <input
            type="checkbox"
            checked={status.autoSyncEnabled !== false}
            disabled={busy !== ""}
            onChange={(event) => {
              const enabled = event.target.checked;
              void execute("auto", async () => {
                setStatus(await saveAutoSyncSettings(enabled));
                return enabled ? "自动同步已开启" : "自动同步已关闭";
              });
            }}
          />
          保存后自动同步到其他设备
        </label>
        <p className="section-intro">
          修改先保存在本机，短时间的多次修改会合并同步。打开应用、回到前台或重新联网时自动检查更新。
        </p>
        {autoState && (
          <div
            role="status"
            className={
              autoState.phase === "attention" ? "error-box" : "sync-live-status"
            }
          >
            {autoState.message}
            {(autoState.phase === "attention" ||
              autoState.phase === "offline") && (
              <button className="secondary" onClick={onRetryAuto}>
                重试同步
              </button>
            )}
          </div>
        )}
        <div className="sync-summary">
          <article>
            <span>本机状态</span>
            <strong>
              {status.localChangedSinceSync ? "有待同步修改" : "与同步基线一致"}
            </strong>
          </article>
          <article>
            <span>上次成功同步</span>
            <strong>
              {status.lastSyncedAt
                ? new Date(status.lastSyncedAt).toLocaleString("zh-CN")
                : "无"}
            </strong>
          </article>
          <article>
            <span>恢复密钥</span>
            <strong>
              {status.hasRecoveryKey ? "已存入钥匙串" : "尚未生成 / 导入"}
            </strong>
          </article>
        </div>
        <details className="payload-details">
          <summary>手动同步与恢复</summary>
          <div className="sync-actions">
            <button
              className="primary"
              onClick={push}
              disabled={busy !== "" || !status.signedIn}
            >
              <Upload size={16} />
              合并并上传
            </button>
            <button
              className="secondary"
              onClick={() => void pull()}
              disabled={
                busy !== "" || !status.signedIn || !status.hasRecoveryKey
              }
            >
              <Download size={16} />
              拉取并合并
            </button>
          </div>
          <details className="payload-details">
            <summary>用云端版本恢复本机</summary>
            <p>
              仅在确认舍弃本机修改时使用。将备份当前本机数据，再用云端版本替换；日常同步请使用上方的合并按钮。
            </p>
            <button
              className="danger-text"
              onClick={() => void pull(true)}
              disabled={
                busy !== "" || !status.signedIn || !status.hasRecoveryKey
              }
            >
              拉取并替换本机数据
            </button>
          </details>
        </details>
        <div className="privacy-boundary">
          {status.privacyBoundary.map((item) => (
            <div key={item}>
              <Check size={13} />
              {item}
            </div>
          ))}
        </div>
      </section>

      <section className="panel form-panel recovery-panel">
        <div className="panel-title">
          <div>
            <h2>恢复密钥</h2>
          </div>
        </div>
        <p className="section-intro">
          云端不保存此密钥。首次上传后从当前设备导出；在新设备登录同一账户后导入，才能解密数据。
        </p>
        {revealedKey && (
          <div className="recovery-value">
            <code>{revealedKey}</code>
            <button className="secondary" onClick={copyRecoveryKey}>
              <Copy size={14} />
              复制
            </button>
          </div>
        )}
        <div className="form-grid single-column">
          <label>
            <span>从其他设备导入恢复密钥</span>
            <input
              type="password"
              value={recoveryKey}
              onChange={(event) => setRecoveryKey(event.target.value)}
              placeholder="mario-sync-v1:…"
            />
          </label>
        </div>
        <div className="form-actions">
          <button
            className="secondary"
            onClick={revealRecoveryKey}
            disabled={busy !== "" || !status.signedIn || !status.hasRecoveryKey}
          >
            显示本机恢复密钥
          </button>
          <button
            className="primary"
            onClick={importRecoveryKey}
            disabled={busy !== "" || !status.signedIn || !recoveryKey}
          >
            保存导入密钥
          </button>
        </div>
      </section>

      {busy && (
        <div className="connection-success">
          <LoaderCircle size={16} className="spin" />
          <span>
            <strong>正在处理</strong>同步期间请不要关闭应用
          </span>
        </div>
      )}
      {error && (
        <div className="error-box">
          <AlertTriangle size={18} />
          {error}
        </div>
      )}
      {result && (
        <div className="connection-success">
          <Check size={16} />
          <span>
            <strong>操作完成</strong>
            {result}
          </span>
        </div>
      )}
    </div>
  );
}
