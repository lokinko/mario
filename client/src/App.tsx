import { DailyTracking } from "./features/portfolio/DailyTracking";
import { startAutoSync, type SyncState } from "./lib/autoSync";
import {
  blockStaleWrites,
  DATA_SAVED,
  CLOUD_CHANGED,
  CLOUD_DATA_UPDATED,
} from "./lib/syncEvents";
import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  Cloud,
  LoaderCircle,
  Menu,
  Settings2,
  X,
} from "lucide-react";
import { autoCloudSync, getModelConfig, getSnapshot } from "./api";
import type { DecisionEntry, ModelConfig, Snapshot } from "./types";
import { checkAndSendReviewReminder } from "./reminders";
import { CloudSync } from "./features/account/CloudSync";
import { DecisionJournal } from "./features/decisions/DecisionJournal";
import { View, initialView, nav, supportingNav } from "./app/navigation";
import type { FoundationSection } from "./app/navigation";
import { Dashboard } from "./features/dashboard/Dashboard";
import { MyFacts } from "./features/portfolio/MyFacts";
import { Foundation } from "./features/portfolio/Foundation";
import { PortfolioLedger } from "./features/portfolio/PortfolioLedger";
import { EvidenceWorkbench } from "./features/research/EvidenceWorkbench";
import { ReviewCenter } from "./features/reviews/ReviewCenter";
import { MemoryCenter } from "./features/memory/MemoryCenter";
import { Advisor } from "./features/research/Advisor";
import { ModelSettings } from "./features/settings/ModelSettings";

function App() {
  const [view, setView] = useState<View>(initialView);
  const [advisorVisited, setAdvisorVisited] = useState(
    () => initialView() === "advisor",
  );
  const [dataRevision, setDataRevision] = useState(0);
  const [foundationSection, setFoundationSection] =
    useState<FoundationSection>("holdings");
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [model, setModel] = useState<ModelConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [startupError, setStartupError] = useState("");
  const [notice, setNotice] = useState("");
  const [decisionDraft, setDecisionDraft] = useState<DecisionEntry | null>(
    null,
  );
  const [analysisToOpen, setAnalysisToOpen] = useState<string | null>(null);
  const [mobileNavOpen, setMobileNavOpen] = useState(false);
  const [modelError, setModelError] = useState("");
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
  const startupRequest = useRef(0);
  const currentView = useRef(view);
  currentView.current = view;
  const edited = useRef(false);
  const [remotePending, setRemotePending] = useState(false);
  const [remoteViewRevision, setRemoteViewRevision] = useState(0);
  const [syncState, setSyncState] = useState<SyncState>({
    phase: "waiting",
    message: "正在检查同步状态",
  });
  const syncController = useRef<ReturnType<typeof startAutoSync> | null>(null);
  const loadRemoteData = async (protectDrafts = false) => {
    const next = await getSnapshot();
    if (
      protectDrafts &&
      edited.current &&
      !["advisor", "cloud", "settings"].includes(currentView.current)
    ) {
      blockStaleWrites(true);
      setRemotePending(true);
      return;
    }
    setSnapshot(next);
    setRemoteViewRevision((value) => value + 1);
    setRemotePending(false);
    blockStaleWrites(false);
    window.dispatchEvent(new Event(CLOUD_DATA_UPDATED));
  };
  useEffect(() => {
    if (loading || startupError) return;
    const controller = startAutoSync({
      sync: autoCloudSync,
      visible: () =>
        document.visibilityState !== "hidden" && document.hasFocus(),
      online: () => navigator.onLine,
      onStatus: setSyncState,
      onRemoteUpdate: () => loadRemoteData(true),
    });
    syncController.current = controller;
    window.addEventListener(DATA_SAVED, controller.changed);
    window.addEventListener(CLOUD_CHANGED, controller.retry);
    window.addEventListener("focus", controller.wake);
    window.addEventListener("blur", controller.wake);
    window.addEventListener("online", controller.wake);
    window.addEventListener("offline", controller.wake);
    document.addEventListener("visibilitychange", controller.wake);
    return () => {
      controller.dispose();
      syncController.current = null;
      window.removeEventListener(DATA_SAVED, controller.changed);
      window.removeEventListener(CLOUD_CHANGED, controller.retry);
      window.removeEventListener("focus", controller.wake);
      window.removeEventListener("blur", controller.wake);
      window.removeEventListener("online", controller.wake);
      window.removeEventListener("offline", controller.wake);
      document.removeEventListener("visibilitychange", controller.wake);
    };
  }, [loading, startupError]);

  const loadApplication = () => {
    setLoading(true);
    setStartupError("");
    const request = ++startupRequest.current;
    getSnapshot()
      .then((nextSnapshot) => {
        if (request === startupRequest.current) setSnapshot(nextSnapshot);
      })
      .catch((error) => {
        if (request === startupRequest.current) setStartupError(String(error));
      })
      .finally(() => {
        if (request === startupRequest.current) setLoading(false);
      });
    getModelConfig()
      .then((nextModel) => {
        if (request === startupRequest.current) {
          setModel(nextModel);
          setModelError("");
        }
      })
      .catch((error) => {
        if (request === startupRequest.current) setModelError(String(error));
      });
  };

  useEffect(() => {
    loadApplication();
    return () => {
      startupRequest.current += 1;
    };
  }, []);

  const refreshInvestmentData = async () => {
    blockStaleWrites(false);
    setRemotePending(false);
    edited.current = false;
    setSnapshot(await getSnapshot());
    setDataRevision((current) => current + 1);
    setDecisionDraft(null);
    setAnalysisToOpen(null);
  };

  useEffect(() => {
    if (!loading && !startupError)
      void checkAndSendReviewReminder().catch(() => undefined);
  }, [loading, startupError]);

  const flash = (message: string) => {
    setNotice(message);
    window.setTimeout(() => setNotice(""), 2400);
  };

  const navigate = (
    nextView: View,
    section: FoundationSection = "holdings",
  ) => {
    edited.current = false;
    setFoundationSection(section);
    setView(nextView);
    if (nextView === "advisor") setAdvisorVisited(true);
    setMobileNavOpen(false);
    window.history.replaceState(null, "", `#${nextView}`);
    window.scrollTo({ top: 0, behavior: "auto" });
  };

  useEffect(() => {
    const followLocation = () => {
      const next = initialView();
      if (next !== currentView.current) {
        edited.current = false;
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
        <span>
          {
            [
              ...nav,
              ...supportingNav,
              { id: "cloud", label: "账户与同步" },
              { id: "settings", label: "模型与隐私" },
            ].find((item) => item.id === view)?.label
          }
        </span>
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
            <img src="/mario-mark.svg" alt="" />
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
          {nav.map((item) => (
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
        </nav>

        <div className="sidebar-spacer" />
        <button
          className={`settings-link ${view === "cloud" ? "active" : ""}`}
          onClick={() => navigate("cloud")}
        >
          <Cloud size={18} /> 账户与同步
        </button>
        <small className="sidebar-sync-state" title={syncState.message}>
          {syncState.phase === "attention"
            ? "同步需要处理 · 打开账户与同步"
            : syncState.message}
        </small>
        <button
          className={`settings-link ${view === "settings" ? "active" : ""}`}
          onClick={() => navigate("settings")}
        >
          <Settings2 size={18} /> 模型与隐私
        </button>
      </aside>

      <main
        inert={mobileNavOpen}
        onChangeCapture={() => {
          edited.current = true;
        }}
      >
        <DailyTracking />
        {supportingNav.some((item) => item.id === view) && (
          <div className="context-breadcrumb">
            <button className="text-button" onClick={() => navigate("facts")}>
              我的情况
            </button>
            <span aria-hidden="true"> / </span>
            <span>{supportingNav.find((item) => item.id === view)?.label}</span>
          </div>
        )}
        {remotePending && (
          <div className="remote-update-notice" role="status">
            其他设备的资料已同步，当前输入暂时保留。
            <button
              className="secondary"
              onClick={() => {
                if (
                  edited.current &&
                  !window.confirm(
                    "查看最新资料会重载当前表单，未保存的输入将丢弃。可先复制需要保留的内容。继续？",
                  )
                )
                  return;
                void loadRemoteData()
                  .then(() => {
                    edited.current = false;
                  })
                  .catch((error) => flash(String(error)));
              }}
            >
              查看最新资料
            </button>
          </div>
        )}
        {notice && (
          <div className="toast">
            <Check size={16} />
            {notice}
          </div>
        )}
        {modelError && (
          <div className="error-box" role="alert">
            模型配置暂时不可用，本地决策与复盘不受影响。
            <button onClick={loadApplication}>重试</button>
          </div>
        )}
        {view === "dashboard" && (
          <Dashboard snapshot={snapshot} navigate={navigate} flash={flash} />
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
        {view === "foundation" && (
          <Foundation
            key={remoteViewRevision}
            initialSection={foundationSection}
            snapshot={snapshot}
            onUpdate={setSnapshot}
            flash={flash}
          />
        )}
        {view === "ledger" && (
          <PortfolioLedger
            key={remoteViewRevision}
            snapshot={snapshot}
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
        {view === "decision" && (
          <DecisionJournal
            key={remoteViewRevision}
            flash={flash}
            seed={decisionDraft}
            clearSeed={() => setDecisionDraft(null)}
            onOpenAnalysis={(id) => {
              setAnalysisToOpen(id);
              navigate("advisor");
            }}
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
              requestedAnalysisId={analysisToOpen}
              clearRequestedAnalysis={() => setAnalysisToOpen(null)}
              onCreateDecisionDraft={(draft) => {
                setDecisionDraft(draft);
                navigate("decision");
              }}
            />
          </div>
        )}
        {view === "cloud" && (
          <CloudSync
            flash={flash}
            onRestore={refreshInvestmentData}
            autoState={syncState}
            onRetryAuto={() => syncController.current?.retry()}
          />
        )}
        {view === "settings" && model && (
          <>
            <ModelSettings model={model} onUpdate={setModel} flash={flash} />
            <details className="page narrow archive-tools">
              <summary>查看已有的资料与记录</summary>
              <div className="archive-links">
                {supportingNav.map((item) => (
                  <button
                    className="secondary"
                    key={item.id}
                    onClick={() => navigate(item.id)}
                  >
                    {item.label}
                  </button>
                ))}
              </div>
            </details>
          </>
        )}
        {(view === "advisor" || view === "settings") && !model && (
          <div className="center-screen">
            <p>
              {modelError
                ? "请先重试读取模型配置。其他本地功能仍可使用。"
                : "正在读取模型配置…"}
            </p>
          </div>
        )}
      </main>
    </div>
  );
}

export default App;
