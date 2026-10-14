import { DailyTracking } from "./features/portfolio/DailyTracking";
import { useApplicationData } from "./app/useApplicationData";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  Cloud,
  LoaderCircle,
  Menu,
  Settings2,
  X,
} from "lucide-react";
import { ServerData } from "./features/account/ServerData";
import { View, initialView, navGroups, viewLabels } from "./app/navigation";
import { MyFacts } from "./features/portfolio/MyFacts";
import { EvidenceWorkbench } from "./features/research/EvidenceWorkbench";
import { ReviewCenter } from "./features/reviews/ReviewCenter";
import { MemoryCenter } from "./features/memory/MemoryCenter";
import { Advisor } from "./features/research/Advisor";
import { ModelSettings } from "./features/settings/ModelSettings";
import { AccountSettings } from "./features/account/WebAccess";
import { PageHeader } from "./components/PageHeader";
import { FloatingExchangeRates } from "./features/portfolio/FloatingExchangeRates";

function App() {
  const [view, setView] = useState<View>(initialView);
  const [advisorVisited, setAdvisorVisited] = useState(
    () => initialView() === "advisor",
  );
  const {
    snapshot,
    setSnapshot,
    model,
    setModel,
    modelError,
    loading,
    startupError,
    remotePending,
    remoteViewRevision,
    dataRevision,
    syncState,
    loadApplication,
    loadRemoteData,
    acceptRemoteData,
    refreshInvestmentData,
    hasDraft,
    markEdited,
    clearDraft,
    retrySync,
  } = useApplicationData(view);
  const [notice, setNotice] = useState("");
  const noticeTimer = useRef<ReturnType<typeof window.setTimeout> | undefined>(
    undefined,
  );
  useEffect(() => () => window.clearTimeout(noticeTimer.current), []);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (!mobileNavOpen) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const focusTimer = window.setTimeout(() =>
      sidebarRef.current?.querySelector<HTMLButtonElement>("button")?.focus(),
    );
    const keyboard = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMobileNavOpen(false);
      if (event.key !== "Tab") return;
      const buttons = Array.from(
        sidebarRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [],
      );
      const first = buttons[0],
        last = buttons.at(-1);
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first?.focus();
      }
    };
    window.addEventListener("keydown", keyboard);
    return () => {
      clearTimeout(focusTimer);
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", keyboard);
      menuTrigger.current?.focus();
    };
  }, [mobileNavOpen]);
  useEffect(() => {
    const resize = () => {
      if (window.innerWidth > 900) setMobileNavOpen(false);
    };
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const currentView = useRef(view);
  currentView.current = view;

  const refreshForNewDay = useCallback(() => {
    void loadRemoteData(true).catch(() => {
      setNotice("今日汇率已检查，页面暂时刷新失败，请稍后重试。");
    });
  }, [loadRemoteData]);

  const flash = (message: string) => {
    window.clearTimeout(noticeTimer.current);
    setNotice(message);
    noticeTimer.current = window.setTimeout(() => setNotice(""), 2400);
  };

  const navigate = (nextView: View) => {
    clearDraft();
    setView(nextView);
    if (nextView === "advisor") setAdvisorVisited(true);
    setMobileNavOpen(false);
    if (currentView.current !== nextView)
      window.history.pushState(null, "", `#${nextView}`);
    window.scrollTo({ top: 0, behavior: "auto" });
  };

  useEffect(() => {
    const followLocation = () => {
      const next = initialView();
      if (next !== currentView.current) {
        clearDraft();
        setView(next);
        if (next === "advisor") setAdvisorVisited(true);
        setMobileNavOpen(false);
        window.scrollTo({ top: 0, behavior: "auto" });
      }
    };
    window.addEventListener("hashchange", followLocation);
    window.addEventListener("popstate", followLocation);
    return () => {
      window.removeEventListener("hashchange", followLocation);
      window.removeEventListener("popstate", followLocation);
    };
  }, []);

  if (loading) {
    return (
      <div className="center-screen">
        <LoaderCircle className="spin" />
        <span>正在加载投资档案…</span>
      </div>
    );
  }

  if (startupError || !snapshot) {
    return (
      <div className="center-screen service-error">
        <AlertTriangle size={28} />
        <strong>服务暂不可用</strong>
        <span>请检查服务是否运行，然后重试。</span>
        <button className="primary" onClick={loadApplication}>
          重新连接
        </button>
        <small>{startupError}</small>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <header className="mobile-topbar" inert={mobileNavOpen}>
        <button
          ref={menuTrigger}
          className="mobile-menu"
          aria-label="打开导航"
          aria-expanded={mobileNavOpen}
          aria-controls="main-navigation"
          onClick={() => setMobileNavOpen(true)}
        >
          <Menu size={22} />
        </button>
        <strong>mario</strong>
        <span>{viewLabels[view]}</span>
      </header>
      {mobileNavOpen && (
        <button
          className="nav-backdrop"
          aria-label="收起导航"
          onClick={() => setMobileNavOpen(false)}
          tabIndex={-1}
        />
      )}
      <aside
        ref={sidebarRef}
        onTransitionEnd={(event) => {
          if (
            mobileNavOpen &&
            event.propertyName === "transform" &&
            !sidebarRef.current?.contains(document.activeElement)
          )
            sidebarRef.current
              ?.querySelector<HTMLButtonElement>("button")
              ?.focus();
        }}
        id="main-navigation"
        aria-label="主导航"
        role={mobileNavOpen ? "dialog" : undefined}
        aria-modal={mobileNavOpen || undefined}
        className={`sidebar ${mobileNavOpen ? "mobile-open" : ""}`}
      >
        <div className="brand">
          <div className="brand-mark">
            <img src={`${import.meta.env.BASE_URL}mario-mark.svg`} alt="" />
          </div>
          <div>
            <strong>mario</strong>
          </div>
          <button
            className="mobile-menu"
            type="button"
            aria-label="关闭导航"
            aria-expanded={mobileNavOpen}
            onClick={() => setMobileNavOpen((open) => !open)}
          >
            <X size={21} />
          </button>
        </div>

        <nav>
          {navGroups.map((group) => (
            <div className="nav-group" key={group.caption}>
              <p className="nav-caption">{group.caption}</p>
              {group.items.map((item) => (
                <button
                  key={item.id}
                  aria-current={view === item.id ? "page" : undefined}
                  className={view === item.id ? "active" : ""}
                  onClick={() => navigate(item.id)}
                >
                  <item.icon size={18} />
                  {item.label}
                </button>
              ))}
            </div>
          ))}
        </nav>

        <div className="sidebar-spacer" />
        <div className="sidebar-footer">
          <button
            className={`settings-link ${view === "cloud" ? "active" : ""}`}
            aria-current={view === "cloud" ? "page" : undefined}
            onClick={() => navigate("cloud")}
          >
            <Cloud size={18} /> 账号与数据
          </button>
          <button
            className={`settings-link ${view === "settings" ? "active" : ""}`}
            aria-current={view === "settings" ? "page" : undefined}
            onClick={() => navigate("settings")}
          >
            <Settings2 size={18} /> 设置
          </button>
          <small className="sidebar-sync-state" title={syncState.message}>
            {syncState.phase === "attention"
              ? "同步需要处理 · 打开账号与数据"
              : syncState.message}
          </small>
        </div>
      </aside>

      <main
        inert={mobileNavOpen}
        onChangeCapture={() => {
          markEdited();
        }}
      >
        <DailyTracking onNewDay={refreshForNewDay} />
        {remotePending && (
          <div className="remote-update-notice" role="status">
            其他设备的资料已同步，当前输入暂时保留。
            <button
              className="secondary"
              onClick={() => {
                if (
                  hasDraft() &&
                  !window.confirm(
                    "查看最新资料会重载当前表单，未保存的输入将丢弃。可先复制需要保留的内容。继续？",
                  )
                )
                  return;
                void acceptRemoteData().catch((error) => flash(String(error)));
              }}
            >
              查看最新资料
            </button>
          </div>
        )}
        {notice && (
          <div className="toast" role="status">
            <Check size={16} />
            {notice}
          </div>
        )}
        {modelError && (
          <div className="error-box" role="alert">
            模型配置暂时不可用，资料与复盘不受影响。
            <button onClick={loadApplication}>重试</button>
          </div>
        )}
        {view === "facts" && (
          <MyFacts
            key={remoteViewRevision}
            snapshot={snapshot}
            onUpdate={setSnapshot}
            navigate={navigate}
            flash={flash}
          />
        )}
        {view === "evidence" && (
          <EvidenceWorkbench
            key={remoteViewRevision}
            navigate={navigate}
            flash={flash}
          />
        )}
        {view === "review" && (
          <ReviewCenter
            key={remoteViewRevision}
            navigate={navigate}
            flash={flash}
          />
        )}
        {view === "memory" && (
          <MemoryCenter key={remoteViewRevision} flash={flash} />
        )}
        {model && advisorVisited && (
          <div hidden={view !== "advisor"}>
            <Advisor
              key={dataRevision}
              active={view === "advisor"}
              model={model}
              navigate={navigate}
            />
          </div>
        )}
        {view === "cloud" && (
          <ServerData
            flash={flash}
            onRestore={refreshInvestmentData}
            autoState={syncState}
            onRetryAuto={retrySync}
          />
        )}
        {view === "settings" && model && (
          <ModelSettings
            accountSettings={<AccountSettings />}
            key={remoteViewRevision}
            model={model}
            onUpdate={setModel}
            flash={flash}
          />
        )}
        {view === "settings" && !model && (
          <div className="page narrow settings-page">
            <PageHeader title="设置" />
            <AccountSettings />
          </div>
        )}
        {(view === "advisor" || view === "settings") && !model && (
          <div className="center-screen">
            <p>
              {modelError
                ? "请先重试读取模型配置。其他功能仍可使用。"
                : "正在读取模型配置…"}
            </p>
          </div>
        )}
      </main>
      {snapshot.profile && (
        <div inert={mobileNavOpen}>
          <FloatingExchangeRates
            baseCurrency={snapshot.profile.baseCurrency}
            usedCurrencies={[
              ...snapshot.holdings.map((holding) => holding.currency),
              ...(snapshot.holdingTrades ?? []).map((trade) => trade.currency),
            ]}
            onUpdate={setSnapshot}
          />
        </div>
      )}
    </div>
  );
}

export default App;
