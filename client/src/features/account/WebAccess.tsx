import {
  createContext,
  Fragment,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { LoaderCircle, LogOut, UsersRound } from "lucide-react";
import {
  forgetWebAccount,
  savedWebAccounts,
  setWebToken,
  switchWebAccount,
  webToken,
  type WebAccount,
} from "../../lib/webSession";
import { requestJson } from "../../lib/transport";
import { acceptRevision, isLocalDev, serviceUrl } from "../../lib/service";
import { blockStaleWrites } from "../../lib/syncEvents";

const AccountAccess = createContext<{
  accounts: WebAccount[];
  connected: string;
  picking: boolean;
  openPicker: () => void;
  closePicker: () => void;
  selectAccount: (token: string) => void;
  addAccount: () => void;
  disconnect: () => void;
  busy: boolean;
} | null>(null);

function SavedAccounts({
  accounts,
  currentToken,
  busy,
  onSelect,
}: {
  accounts: WebAccount[];
  currentToken?: string;
  busy: boolean;
  onSelect: (token: string) => void;
}) {
  return (
    <div
      className="form-grid single-column"
      role="group"
      aria-label="已登录账号"
    >
      {accounts.map((account) => (
        <button
          type="button"
          className="secondary"
          key={account.token}
          disabled={busy || currentToken === account.token}
          onClick={() => onSelect(account.token)}
        >
          <UsersRound size={16} />
          {currentToken === account.token ? "当前账号：" : "切换到 "}
          {account.email || "已有账号"}
        </button>
      ))}
    </div>
  );
}

export function AccountSettings() {
  const account = useContext(AccountAccess);
  if (!account) return null;
  return (
    <section className="panel form-panel" aria-label="账号">
      <div className="panel-title">
        <h2>账号</h2>
      </div>
      <p className="section-intro">
        已登录账号可直接切换，无需重复输入密码。切换时会清除未保存的输入，已保存的资料保留在各自账号中。
      </p>
      {account.picking ? (
        <>
          <SavedAccounts
            accounts={account.accounts}
            currentToken={account.connected}
            busy={account.busy}
            onSelect={account.selectAccount}
          />
          <div className="form-actions">
            <button
              type="button"
              className="secondary"
              disabled={account.busy}
              onClick={account.addAccount}
            >
              登录其他账号
            </button>
            <button
              type="button"
              className="text-button"
              disabled={account.busy}
              onClick={account.closePicker}
            >
              取消切换
            </button>
          </div>
        </>
      ) : null}
      <div className="form-actions">
        {!account.picking && (
          <button
            type="button"
            className="secondary"
            disabled={account.busy}
            onClick={account.openPicker}
          >
            <UsersRound size={16} />
            切换账号
          </button>
        )}
        <button
          type="button"
          className="secondary"
          disabled={account.busy}
          onClick={account.disconnect}
        >
          {account.busy ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <LogOut size={16} />
          )}
          退出账号
        </button>
      </div>
    </section>
  );
}

export function WebAccess({ children }: { children: ReactNode }) {
  const [connected, setConnected] = useState(webToken);
  const [accounts, setAccounts] = useState(savedWebAccounts);
  const [picking, setPicking] = useState(false);
  const [addingAccount, setAddingAccount] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [register, setRegister] = useState(false);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const disconnecting = useRef(false);
  const resetAccountState = () => {
    acceptRevision();
    blockStaleWrites(false);
    for (const key of [
      "mario.advisorQuestion",
      "compass.advisorQuestion",
      "mario.dailyAssetRange",
    ])
      sessionStorage.removeItem(key);
    setEmail("");
    setPassword("");
    setRegister(false);
    setError("");
  };
  const clear = () => {
    const token = webToken();
    if (token) forgetWebAccount(token);
    else setWebToken();
    resetAccountState();
    setConnected(undefined);
    setAccounts(savedWebAccounts());
    setAddingAccount(false);
    setPicking(false);
  };
  const selectAccount = (token: string) => {
    if (busy) return;
    if (!switchWebAccount(token)) {
      setAccounts(savedWebAccounts());
      setError("该账号的登录已过期，请重新登录。");
      return;
    }
    resetAccountState();
    setConnected(token);
    setAccounts(savedWebAccounts());
    setAddingAccount(false);
    setPicking(false);
  };
  const disconnect = async () => {
    if (disconnecting.current || !connected) return;
    disconnecting.current = true;
    setBusy(true);
    setError("");
    try {
      await requestJson(`${serviceUrl()}/auth/logout`, {
        method: "POST",
        headers: { Authorization: `Bearer ${connected}` },
      });
      const currentToken = webToken();
      forgetWebAccount(connected);
      if (!currentToken || currentToken === connected) clear();
      else setAccounts(savedWebAccounts());
    } catch {
      if (webToken() === connected)
        setError("退出失败，请联网后重试以撤销服务端会话。");
    } finally {
      disconnecting.current = false;
      setBusy(false);
    }
  };
  useEffect(() => {
    const expire = () => {
      clear();
      setError("该账号的登录已过期，请重新登录，或选择其他已登录账号。");
    };
    window.addEventListener("mario:session-expired", expire);
    return () => window.removeEventListener("mario:session-expired", expire);
  }, []);
  useEffect(() => {
    const restore = (event: StorageEvent) => {
      if (event.storageArea !== localStorage) return;
      const token = webToken();
      setAccounts(savedWebAccounts());
      if (token === connected) return;
      resetAccountState();
      setConnected(token);
      setAddingAccount(false);
      setPicking(false);
    };
    window.addEventListener("storage", restore);
    return () => window.removeEventListener("storage", restore);
  }, [connected]);
  if (isLocalDev()) return children;
  if (connected && !addingAccount)
    return (
      <AccountAccess.Provider
        value={{
          accounts,
          connected,
          picking,
          busy,
          disconnect: () => void disconnect(),
          openPicker: () => {
            setAccounts(savedWebAccounts());
            setPicking(true);
            setError("");
          },
          closePicker: () => setPicking(false),
          selectAccount,
          addAccount: () => {
            resetAccountState();
            setAddingAccount(true);
          },
        }}
      >
        {error && <p role="alert">{error}</p>}
        <Fragment key={connected}>{children}</Fragment>
      </AccountAccess.Provider>
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
          const startingToken = webToken();
          const startingServer = serviceUrl();
          try {
            const base = serviceUrl();
            const apiUrl = new URL(base, location.href);
            if (
              !["localhost", "127.0.0.1", "[::1]"].includes(apiUrl.hostname) &&
              apiUrl.protocol !== "https:"
            )
              throw new Error("远程访问请使用 HTTPS");
            const result = await requestJson<{
              token: string;
              expiresAt: number;
              email?: string;
              userId?: string;
            }>(`${base}/auth/${register ? "register" : "login"}`, {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({
                email,
                password,
              }),
            });
            // Another tab may have switched accounts while login was in flight.
            if (webToken() !== startingToken || serviceUrl() !== startingServer)
              return;
            setWebToken(result.token, result.expiresAt, {
              email: result.email || email,
              userId: result.userId,
            });
            resetAccountState();
            setConnected(result.token);
            setAccounts(savedWebAccounts());
            setAddingAccount(false);
            setPicking(false);
          } catch (error) {
            setError(error instanceof Error ? error.message : String(error));
          } finally {
            setBusy(false);
          }
        }}
      >
        <img
          src={`${import.meta.env.BASE_URL}mario-mark.svg`}
          width="48"
          height="48"
          alt=""
        />
        <h1>{register ? "创建 mario 账号" : "登录 mario"}</h1>
        {accounts.length > 0 && (
          <>
            <p>选择已登录账号，或登录其他账号。</p>
            <SavedAccounts
              accounts={accounts}
              busy={busy}
              onSelect={selectAccount}
            />
          </>
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
        <p>资料存储在你的服务器，同一账号可在不同设备的浏览器访问。</p>
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
          className="text-button"
          disabled={busy}
          onClick={() => {
            setRegister(!register);
            setError("");
          }}
        >
          {register ? "已有账号，去登录" : "注册新账号"}
        </button>
        {connected && (
          <button
            type="button"
            className="text-button"
            disabled={busy}
            onClick={() => {
              selectAccount(connected);
            }}
          >
            返回当前账号
          </button>
        )}
      </form>
    </div>
  );
}
