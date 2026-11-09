import { currentSession } from "../../lib/webSession";
import { Callout } from "../../components/Callout";
import { Button, Input } from "antd";
import {
  createContext,
  Fragment,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import {
  ChartNoAxesCombined,
  ArrowUpRight,
  BookOpen,
  LoaderCircle,
  LogOut,
  UsersRound,
} from "lucide-react";
import {
  forgetWebAccount,
  savedWebAccounts,
  setWebToken,
  switchWebAccount,
  webToken,
  type WebAccount,
} from "../../lib/webSession";
import { requestJson } from "../../lib/transport";
import { serviceUrl } from "../../lib/service";

import { OpenAiSignIn } from "./OpenAiSignIn";
import { completeOpenAiSignIn, type OpenAiLoginResult } from "./openaiLogin";

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
      <OpenAiSignIn link disabled={account.busy} />
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
  const [notice, setNotice] = useState("");
  const [openAiReturn] = useState(() =>
    new URLSearchParams(window.location.hash.slice(1)).get("openai-login"),
  );
  const [completingOpenAi, setCompletingOpenAi] = useState(
    openAiReturn === "complete",
  );
  const openAiCompletion = useRef<Promise<OpenAiLoginResult> | null>(null);
  const disconnecting = useRef(false);
  useEffect(() => {
    if (!openAiReturn) return;
    history.replaceState(null, "", location.pathname + location.search);
    if (openAiReturn !== "complete") {
      setError(
        openAiReturn === "conflict"
          ? "邮箱或 ChatGPT 身份已有账号关联。请先用原 Mario 账号登录，再在账号设置关联或核对。"
          : "ChatGPT 登录未完成、已取消或已过期，请重新发起。",
      );
      return;
    }
    let disposed = false;
    setBusy(true);
    // React StrictMode reattaches to the same one-use completion request.
    openAiCompletion.current ??= completeOpenAiSignIn();
    void openAiCompletion.current
      .then(
        ({ session }) => {
          if (disposed) return;
          if (session) {
            setWebToken(session.token, session.expiresAt, session);
            resetAccountState();
            setConnected(session.token);
            setAccounts(savedWebAccounts());
            setAddingAccount(false);
            setPicking(false);
          } else
            setNotice("已关联 ChatGPT，下次可直接使用 ChatGPT 登录 Mario。");
        },
        () => {
          if (!disposed)
            setError(
              "无法完成 ChatGPT 登录交接，账号可能已切换或交接已过期，请重新登录。",
            );
        },
      )
      .finally(() => {
        if (!disposed) {
          setBusy(false);
          setCompletingOpenAi(false);
        }
      });
    return () => {
      disposed = true;
    };
  }, []);
  const resetAccountState = () => {
    currentSession().acceptRevision();
    currentSession().blockStaleWrites(false);
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
  if (completingOpenAi)
    return (
      <div className="web-access">
        <p role="status">正在完成 ChatGPT 登录…</p>
      </div>
    );
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
        {notice && <p role="status">{notice}</p>}
        <Fragment key={connected}>{children}</Fragment>
      </AccountAccess.Provider>
    );
  return (
    <div className="web-access">
      <aside className="login-brand">
        <div className="login-wordmark">
          <img
            src={`${import.meta.env.BASE_URL}mario-mark.svg`}
            width="36"
            height="36"
            alt=""
          />
          <strong>mario</strong>
        </div>
        <div className="login-story">
          <span className="login-kicker">你的个人投资工作台</span>
          <h2>
            让每一次判断，
            <br />
            都有据可依。
          </h2>
          <p>
            照看认可的公司，记录真实的交易。
            <br />
            在长期观察中，形成自己的投资判断。
          </p>
          <div className="login-preview" aria-hidden="true">
            <div>
              <ChartNoAxesCombined size={20} />
              <span>观察 · 等待 · 复盘</span>
              <ArrowUpRight size={18} />
            </div>
            <div className="login-preview-chart">
              <svg viewBox="0 0 360 120" fill="none">
                <path
                  className="login-chart-grid"
                  d="M0 30H360M0 60H360M0 90H360"
                />
                <path
                  className="login-chart-line"
                  d="M0 92L30 82L60 88L90 60L120 70L150 45L180 53L210 30L240 46L270 25L300 35L330 16L360 22"
                />
              </svg>
            </div>
            <span>核对依据，记录判断，先看风险。</span>
          </div>
        </div>
        <div className="login-footnote">
          <BookOpen size={15} /> 让耐心成为投资的一部分
        </div>
      </aside>
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
        <div className="login-heading">
          <span className="login-kicker">欢迎来到你的投资工作台</span>
          <h1>{register ? "创建 mario 账号" : "登录 mario"}</h1>
          <p>从今天的观察，继续长期的积累。</p>
        </div>
        <OpenAiSignIn disabled={busy} />
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
          <Input
            type="email"
            placeholder="输入邮箱地址"
            required
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </label>
        <label>
          密码
          <Input
            type="password"
            placeholder="输入密码（至少 12 位）"
            required
            minLength={12}
            maxLength={256}
            autoComplete={register ? "new-password" : "current-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <p>资料存储在你的服务器，同一账号可在不同设备的浏览器访问。</p>
        {error && <Callout tone="danger">{error}</Callout>}
        <Button
          type="primary"
          autoInsertSpace={false}
          htmlType="submit"
          disabled={busy}
          loading={busy}
          block
        >
          {busy ? "连接中…" : register ? "创建账号" : "登录"}
        </Button>
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
