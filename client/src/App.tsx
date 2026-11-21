import { Callout } from "./components/Callout";
import { DailyTracking } from "./features/portfolio/DailyTracking";
import { useApplicationData } from "./app/useApplicationData";
import { lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { DraftProtection } from "./components/DraftProtection";
import {
  AlertTriangle,
  Check,
  Ellipsis,
  Settings2,
  XCircle,
  X,
  ChevronRight,
} from "lucide-react";
import {
  View,
  initialView,
  mobileTabs,
  navGroups,
  viewLabels,
} from "./app/navigation";
import type { FactsSection } from "./features/portfolio/MyFacts";
import { DeferredPage } from "./components/DeferredPage";
import { PageHeader } from "./components/PageHeader";
import { AmountPrivacy } from "./components/AmountPrivacy";
import { useDisplayPreferences } from "./app/useDisplayPreferences";
import { DisplayPreferences } from "./components/DisplayPreferences";
import { ApplicationSkeleton } from "./components/Skeleton";
import { useActivityTracking } from "./app/useActivityTracking";
import type { ReferenceTab } from "./app/ReferenceInfo";
import {
  dividendQuestionParam,
  dividendQuestionSymbol,
} from "./features/watch/dividend-preview/route";

const BuyWatch = lazy(() =>
  import("./features/watch/BuyWatch").then((m) => ({ default: m.BuyWatch })),
);
const ReferenceInfo = lazy(() =>
  import("./app/ReferenceInfo").then((m) => ({ default: m.ReferenceInfo })),
);
const MyFacts = lazy(() =>
  import("./features/portfolio/MyFacts").then((m) => ({ default: m.MyFacts })),
);
const DividendCalendar = lazy(() =>
  import("./features/watch/DividendCalendar").then((m) => ({
    default: m.DividendCalendar,
  })),
);
const Advisor = lazy(() =>
  import("./features/advisor/Advisor").then((m) => ({ default: m.Advisor })),
);
const InvestmentMethodology = lazy(() =>
  import("./features/methodology/InvestmentMethodology").then((m) => ({
    default: m.InvestmentMethodology,
  })),
);
const ModelSettings = lazy(() =>
  import("./features/settings/ModelSettings").then((m) => ({
    default: m.ModelSettings,
  })),
);
const ServerData = lazy(() =>
  import("./features/account/ServerData").then((m) => ({
    default: m.ServerData,
  })),
);

const syncLabels = {
  waiting: "等待同步",
  syncing: "正在同步",
  synced: "已同步",
  offline: "连接中断",
  paused: "同步暂停",
  attention: "同步需处理",
};

function App() {
  const display = useDisplayPreferences();
  const [view, setView] = useState<View>(initialView);
  const [referenceTab, setReferenceTab] = useState<ReferenceTab>("evidence");
  useActivityTracking(
    view !== "reference"
      ? view
      : referenceTab === "evidence"
        ? "evidence"
        : "review",
  );
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
  const draftProtection = useMemo(
    () => ({ markEdited, clearDraft }),
    [markEdited, clearDraft],
  );
  const [pendingQuestion, setPendingQuestion] = useState<
    { id: number; question: string } | undefined
  >(() => {
    const symbol = dividendQuestionSymbol(window.location.search);
    if (!symbol || view !== "advisor") return undefined;
    return {
      id: 0,
      question: `请研究 A 股 ${symbol} 的现金分红。核对最新公司公告，分别列出已实施分红和仍在预案阶段的分红。按同一报告期比较每 10 股税前派息，并结合财报查分红变化的原因、现金来源和可持续性。注明数据日期、出处和尚待核查的问题。`,
    };
  });
  const questionId = useRef(0);
  const acceptQuestion = useCallback((id: number) => {
    if (id === 0) {
      const url = new URL(window.location.href);
      url.searchParams.delete(dividendQuestionParam);
      url.hash = "advisor";
      window.history.replaceState(null, "", url);
    }
    setPendingQuestion((current) => (current?.id === id ? undefined : current));
  }, []);
  const [notice, setNotice] = useState("");
  const [noticeTone, setNoticeTone] = useState<"success" | "danger">("success");
  const noticeTimer = useRef<ReturnType<typeof window.setTimeout> | undefined>(
    undefined,
  );
  useEffect(() => () => window.clearTimeout(noticeTimer.current), []);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [factsSection, setFactsSection] = useState<FactsSection>("holdings");
  const [watchTarget, setWatchTarget] = useState<
    { id: number; symbol: string; personal: boolean } | undefined
  >(undefined);
  const watchTargetId = useRef(0);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  const sidebarRef = useRef<HTMLElement>(null);
  const mainRef = useRef<HTMLElement>(null);
  const focusAfterNavigation = useRef(false);
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
      const controls = Array.from(
        sidebarRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [],
      );
      const first = controls[0],
        last = controls.at(-1);
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
      window.clearTimeout(noticeTimer.current);
      setNoticeTone("danger");
      setNotice("今日汇率已检查，页面暂时刷新失败，请稍后重试。");
    });
  }, [loadRemoteData]);

  const flash = (message: string, tone: "success" | "danger" = "success") => {
    setNoticeTone(tone);
    window.clearTimeout(noticeTimer.current);
    setNotice(message);
    if (tone === "success")
      noticeTimer.current = window.setTimeout(() => setNotice(""), 2400);
  };

  const changeView = useCallback(
    (nextView: View) => {
      if (nextView === currentView.current) return true;
      if (
        currentView.current !== "advisor" &&
        hasDraft() &&
        !window.confirm("有未保存的修改，离开当前页面会丢弃输入。确认离开？")
      )
        return false;
      clearDraft();
      // History navigation may deliver both popstate and hashchange before render.
      currentView.current = nextView;
      focusAfterNavigation.current = true;
      setView(nextView);
      if (nextView === "advisor") setAdvisorVisited(true);
      setMobileNavOpen(false);
      window.scrollTo({ top: 0, behavior: "auto" });
      return true;
    },
    [clearDraft, hasDraft],
  );

  const navigate = (nextView: View) => {
    const changed = currentView.current !== nextView;
    if (!changeView(nextView)) return false;
    setMobileNavOpen(false);
    if (changed) window.history.pushState(null, "", `#${nextView}`);
    return true;
  };

  const startAnalysis = (question: string) => {
    if (!navigate("advisor")) return;
    setPendingQuestion({ id: ++questionId.current, question });
  };

  const openWatchSymbol = (symbol: string, personal: boolean) => {
    if (!navigate("watch")) return;
    setWatchTarget({ id: ++watchTargetId.current, symbol, personal });
  };

  useEffect(() => {
    const followLocation = () => {
      if (!changeView(initialView()))
        window.history.replaceState(null, "", `#${currentView.current}`);
    };
    window.addEventListener("hashchange", followLocation);
    window.addEventListener("popstate", followLocation);
    return () => {
      window.removeEventListener("hashchange", followLocation);
      window.removeEventListener("popstate", followLocation);
    };
  }, [changeView]);

  useEffect(() => {
    if (!focusAfterNavigation.current || loading || !mainRef.current) return;
    focusAfterNavigation.current = false;
    mainRef.current.focus({ preventScroll: true });
  }, [view, loading]);

  if (loading) {
    return <ApplicationSkeleton />;
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

  const content = (
    <div className="app-shell">
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
            <span>个人投资工作台</span>
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
                  className={[
                    view === item.id ? "active" : "",
                    mobileTabs.some((tab) => tab.id === item.id)
                      ? "in-tabbar"
                      : "",
                  ].join(" ")}
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
            className={`settings-link ${view === "settings" ? "active" : ""}`}
            aria-current={view === "settings" ? "page" : undefined}
            onClick={() => navigate("settings")}
          >
            <Settings2 size={18} /> 设置
          </button>
        </div>
      </aside>

      <main
        ref={mainRef}
        tabIndex={-1}
        aria-label={viewLabels[view]}
        inert={mobileNavOpen}
        onChangeCapture={(event) => {
          if (
            !(event.target as HTMLElement).closest(
              "[data-presentation], [data-draft-scope]",
            )
          )
            markEdited();
        }}
      >
        <header className="workspace-bar workspace-context">
          <div className="workspace-location">
            <span className="workspace-home">个人工作台</span>
            <img
              className="workspace-mark"
              src={`${import.meta.env.BASE_URL}mario-mark.svg`}
              alt=""
              width="26"
              height="26"
            />
            <ChevronRight size={14} aria-hidden="true" />
            <strong>{viewLabels[view]}</strong>
          </div>
          <button
            className={`workspace-sync sync-${syncState.phase}`}
            type="button"
            aria-label={`${syncState.message}，打开设置`}
            title={syncState.message}
            onClick={() => navigate("settings")}
          >
            <span className="sync-indicator" aria-hidden="true" />
            {syncLabels[syncState.phase]}
          </button>
        </header>
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
                void acceptRemoteData().catch((error) =>
                  flash(String(error), "danger"),
                );
              }}
            >
              查看最新资料
            </button>
          </div>
        )}
        {notice && (
          <div
            className="toast"
            role={noticeTone === "danger" ? "alert" : "status"}
          >
            {noticeTone === "danger" ? (
              <XCircle size={16} />
            ) : (
              <Check size={16} />
            )}
            {notice}
            <button
              className="icon-button"
              aria-label="关闭消息"
              onClick={() => setNotice("")}
            >
              <X size={16} />
            </button>
          </div>
        )}
        {modelError && (view === "advisor" || view === "settings") && (
          <Callout tone="danger">
            模型配置暂时不可用，资料与复盘不受影响。
            <button onClick={loadApplication}>重试</button>
          </Callout>
        )}
        {view !== "advisor" && (
          <DeferredPage key={view}>
            {view === "watch" && (
              <BuyWatch
                key={remoteViewRevision}
                snapshot={snapshot}
                onAsk={startAnalysis}
                flash={flash}
                onEdited={markEdited}
                onSaved={clearDraft}
                target={watchTarget}
                onTargetConsumed={(id) =>
                  setWatchTarget((current) =>
                    current?.id === id ? undefined : current,
                  )
                }
              />
            )}
            {view === "facts" && (
              <MyFacts
                key={remoteViewRevision}
                snapshot={snapshot}
                onUpdate={setSnapshot}
                display={display}
                flash={flash}
                section={factsSection}
                onSectionChange={setFactsSection}
                onSaved={clearDraft}
              />
            )}
            {view === "reference" && (
              <ReferenceInfo
                key={remoteViewRevision}
                tab={referenceTab}
                onTabChange={setReferenceTab}
                snapshot={snapshot}
                onAsk={startAnalysis}
                flash={flash}
              />
            )}
            {view === "calendar" && (
              <DividendCalendar
                key={remoteViewRevision}
                onAsk={startAnalysis}
                onInspectWatch={openWatchSymbol}
              />
            )}
            {view === "methodology" && (
              <InvestmentMethodology
                key={remoteViewRevision}
                flash={flash}
                onSaved={clearDraft}
                onEdited={markEdited}
              />
            )}
            {view === "settings" && model && (
              <ModelSettings
                displaySettings={<DisplayPreferences {...display} />}
                accountSettings={
                  <ServerData
                    flash={flash}
                    onRestore={refreshInvestmentData}
                    autoState={syncState}
                    onRetryAuto={retrySync}
                  />
                }
                key={remoteViewRevision}
                model={model}
                onUpdate={setModel}
                onSaved={clearDraft}
                flash={flash}
              />
            )}
            {view === "settings" && !model && (
              <div className="page narrow settings-page">
                <PageHeader title="设置" />
                <nav className="settings-directory" aria-label="设置分区">
                  <button
                    type="button"
                    onClick={() =>
                      document
                        .getElementById("account-settings")
                        ?.scrollIntoView({ behavior: "smooth" })
                    }
                  >
                    账号与数据
                  </button>
                  <button
                    type="button"
                    onClick={() =>
                      document
                        .getElementById("display-settings")
                        ?.scrollIntoView({ behavior: "smooth" })
                    }
                  >
                    显示偏好
                  </button>
                </nav>
                <ServerData
                  flash={flash}
                  onRestore={refreshInvestmentData}
                  autoState={syncState}
                  onRetryAuto={retrySync}
                />
                <DisplayPreferences {...display} />
              </div>
            )}
          </DeferredPage>
        )}
        {model && advisorVisited && (
          <div hidden={view !== "advisor"}>
            <DeferredPage>
              <Advisor
                key={dataRevision}
                active={view === "advisor"}
                model={model}
                onOpenSettings={() => navigate("settings")}
                pendingQuestion={pendingQuestion}
                onQuestionAccepted={acceptQuestion}
              />
            </DeferredPage>
          </div>
        )}
        {view === "advisor" && !model && !modelError && (
          <div className="center-screen">
            <p role="status">正在读取模型配置…</p>
          </div>
        )}
      </main>
      <nav
        className="mobile-tabbar"
        aria-label="快捷导航"
        inert={mobileNavOpen}
      >
        {mobileTabs.map((tab) => {
          const active = view === tab.id;
          return (
            <button
              key={tab.id}
              className={active ? "active" : ""}
              aria-current={active ? "page" : undefined}
              onClick={() => navigate(tab.id)}
            >
              <tab.icon size={22} aria-hidden="true" />
              <span>{tab.label}</span>
            </button>
          );
        })}
        <button
          ref={menuTrigger}
          className={mobileTabs.some((tab) => tab.id === view) ? "" : "active"}
          aria-label="更多"
          aria-expanded={mobileNavOpen}
          aria-controls="main-navigation"
          onClick={() => setMobileNavOpen(true)}
        >
          <Ellipsis size={22} aria-hidden="true" />
          <span>更多</span>
        </button>
      </nav>
    </div>
  );
  return (
    <DraftProtection.Provider value={draftProtection}>
      <AmountPrivacy.Provider value={display.maskAmounts}>
        {content}
      </AmountPrivacy.Provider>
    </DraftProtection.Provider>
  );
}

export default App;
