import { BuyWatch } from "./features/watch/BuyWatch";
import { Breadcrumb, Button } from "antd";
import { Callout } from "./components/Callout";
import { DailyTracking } from "./features/portfolio/DailyTracking";
import { useApplicationData } from "./app/useApplicationData";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  Cloud,
  Ellipsis,
  Settings2,
  XCircle,
  X,
  CircleUserRound,
  PanelLeft,
} from "lucide-react";
import { ServerData } from "./features/account/ServerData";
import {
  View,
  initialView,
  mobileTabs,
  navGroups,
  viewLabels,
} from "./app/navigation";
import { MyFacts, type FactsSection } from "./features/portfolio/MyFacts";
import { EvidenceWorkbench } from "./features/research/EvidenceWorkbench";
import { ReviewCenter } from "./features/reviews/ReviewCenter";
import { Advisor } from "./features/research/Advisor";
import { InvestmentMethodology } from "./features/methodology/InvestmentMethodology";
import { ModelSettings } from "./features/settings/ModelSettings";
import { PageHeader } from "./components/PageHeader";
import { ExchangeRateAccess } from "./features/portfolio/ExchangeRateAccess";
import { useDisplayPreferences } from "./app/useDisplayPreferences";
import { DisplayPreferences } from "./components/DisplayPreferences";
import { ApplicationSkeleton } from "./components/Skeleton";

function App() {
  const display = useDisplayPreferences();
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
  const [pendingQuestion, setPendingQuestion] = useState<{
    id: number;
    question: string;
  }>();
  const questionId = useRef(0);
  const acceptQuestion = useCallback((id: number) => {
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
      setView(nextView);
      if (nextView === "advisor") setAdvisorVisited(true);
      setMobileNavOpen(false);
      window.scrollTo({ top: 0, behavior: "auto" });
      return true;
    },
    [clearDraft, hasDraft],
  );

  const navigate = (nextView: View, section?: FactsSection) => {
    const changed = currentView.current !== nextView;
    if (!changeView(nextView)) return false;
    if (section) setFactsSection(section);
    setMobileNavOpen(false);
    if (changed) window.history.pushState(null, "", `#${nextView}`);
    return true;
  };

  const startAnalysis = (question: string) => {
    if (!navigate("advisor")) return;
    setPendingQuestion({ id: ++questionId.current, question });
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

  return (
    <div className="app-shell">
      <header className="mobile-topbar" inert={mobileNavOpen}>
        <img
          className="mobile-mark"
          src={`${import.meta.env.BASE_URL}mario-mark.svg`}
          alt=""
        />
        <strong>mario</strong>
        <span>{viewLabels[view]}</span>
        <button
          className={`mobile-sync sync-${syncState.phase}`}
          aria-label={syncState.message}
          onClick={() => navigate("cloud")}
        >
          <span className="sync-indicator" />
        </button>
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
                    mobileTabs.some((tab) => tab.view === item.id)
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
          <button
            className={`sidebar-sync-state sync-${syncState.phase}`}
            title={syncState.message}
            onClick={() => navigate("cloud")}
          >
            <span className="sync-indicator" aria-hidden="true" />
            {syncState.phase === "attention"
              ? "同步需要处理 · 打开账号与数据"
              : syncState.message}
          </button>
          {snapshot.profile && (
            <ExchangeRateAccess snapshot={snapshot} onUpdate={setSnapshot} />
          )}
        </div>
      </aside>

      <main
        inert={mobileNavOpen}
        onChangeCapture={(event) => {
          if (!(event.target as HTMLElement).closest("[data-presentation]"))
            markEdited();
        }}
      >
        <div className="workspace-bar">
          <div className="workspace-location">
            <PanelLeft size={16} aria-hidden="true" />
            <Breadcrumb
              items={[{ title: "个人工作台" }, { title: viewLabels[view] }]}
            />
          </div>
          <Button
            type="text"
            icon={<CircleUserRound size={17} />}
            onClick={() => navigate("cloud")}
          >
            我的账号
          </Button>
        </div>
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
        {modelError && (
          <Callout tone="danger">
            模型配置暂时不可用，资料与复盘不受影响。
            <button onClick={loadApplication}>重试</button>
          </Callout>
        )}
        {view === "watch" && (
          <BuyWatch
            key={remoteViewRevision}
            snapshot={snapshot}
            onAsk={startAnalysis}
            flash={flash}
            onEdited={markEdited}
            onSaved={clearDraft}
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
          />
        )}
        {view === "evidence" && (
          <EvidenceWorkbench
            key={remoteViewRevision}
            onAsk={startAnalysis}
            flash={flash}
          />
        )}
        {view === "review" && (
          <ReviewCenter
            key={remoteViewRevision}
            onAsk={startAnalysis}
            flash={flash}
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
        {model && advisorVisited && (
          <div hidden={view !== "advisor"}>
            <Advisor
              key={dataRevision}
              active={view === "advisor"}
              model={model}
              onOpenSettings={() => navigate("settings")}
              pendingQuestion={pendingQuestion}
              onQuestionAccepted={acceptQuestion}
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
            displaySettings={<DisplayPreferences {...display} />}
            key={remoteViewRevision}
            model={model}
            onUpdate={setModel}
            flash={flash}
          />
        )}
        {view === "settings" && !model && (
          <div className="page narrow settings-page">
            <PageHeader title="设置" />
            <DisplayPreferences {...display} />
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
      <nav
        className="mobile-tabbar"
        aria-label="快捷导航"
        inert={mobileNavOpen}
      >
        {mobileTabs.map((tab) => {
          const active =
            view === tab.view && (!tab.section || tab.section === factsSection);
          return (
            <button
              key={tab.id}
              className={active ? "active" : ""}
              aria-current={active ? "page" : undefined}
              onClick={() => navigate(tab.view, tab.section)}
            >
              <tab.icon size={22} aria-hidden="true" />
              <span>{tab.label}</span>
            </button>
          );
        })}
        <button
          ref={menuTrigger}
          className={
            mobileTabs.some((tab) => tab.view === view) ? "" : "active"
          }
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
}

export default App;
