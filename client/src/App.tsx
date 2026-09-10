import { useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  Check,
  Cloud,
  LoaderCircle,
  LockKeyhole,
  Menu,
  Settings2,
  X,
} from "lucide-react";
import { getModelConfig, getSnapshot } from "./api";
import type { DecisionEntry, ModelConfig, Snapshot } from "./types";
import { checkAndSendReviewReminder } from "./reminders";
import { CloudSync } from "./features/account/CloudSync";
import { DecisionJournal } from "./features/decisions/DecisionJournal";
import { View, initialView, nav, supportingNav } from "./app/navigation";
import type { FoundationSection } from "./app/navigation";
import { Dashboard } from "./features/dashboard/Dashboard";
import { Foundation } from "./features/portfolio/Foundation";
import { PortfolioLedger } from "./features/portfolio/PortfolioLedger";
import { EvidenceWorkbench } from "./features/research/EvidenceWorkbench";
import { ReviewCenter } from "./features/reviews/ReviewCenter";
import { MemoryCenter } from "./features/memory/MemoryCenter";
import { Advisor } from "./features/research/Advisor";
import { ModelSettings } from "./features/settings/ModelSettings";

function App() {
  const [view, setView] = useState<View>(initialView);
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
  const startupRequest = useRef(0);

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
    setSnapshot(await getSnapshot());
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
    setFoundationSection(section);
    setView(nextView);
    setMobileNavOpen(false);
    window.history.replaceState(null, "", `#${nextView}`);
    window.scrollTo({ top: 0, behavior: "auto" });
  };

  if (loading) {
    return (
      <div className="center-screen">
        <LoaderCircle className="spin" />
        <span>正在加载本地投资档案…</span>
      </div>
    );
  }

  if (startupError || !snapshot) {
    return (
      <div className="center-screen service-error">
        <AlertTriangle size={28} />
        <strong>本地服务尚未就绪</strong>
        <span>客户端没有连接到本次启动的本地服务。你的数据没有丢失。</span>
        <button className="primary" onClick={loadApplication}>
          重新连接
        </button>
        <small>{startupError}</small>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <aside className={`sidebar ${mobileNavOpen ? "mobile-open" : ""}`}>
        <div className="brand">
          <div className="brand-mark">
            <img src="/mario-mark.svg" alt="" />
          </div>
          <div>
            <strong>mario</strong>
            <span>本地投资决策助手</span>
          </div>
          <button
            className="mobile-menu"
            type="button"
            aria-label={mobileNavOpen ? "关闭导航" : "打开导航"}
            aria-expanded={mobileNavOpen}
            onClick={() => setMobileNavOpen((open) => !open)}
          >
            {mobileNavOpen ? <X size={21} /> : <Menu size={21} />}
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
          <details
            className="supporting-nav"
            open={supportingNav.some((item) => item.id === view)}
          >
            <summary>资料与工具</summary>
            {supportingNav.map((item) => (
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
          </details>
        </nav>

        <div className="sidebar-spacer" />
        <div className="privacy-card">
          <LockKeyhole size={18} />
          <div>
            <strong>本地优先</strong>
          </div>
        </div>
        <button
          className={`settings-link ${view === "cloud" ? "active" : ""}`}
          onClick={() => navigate("cloud")}
        >
          <Cloud size={18} /> 账户与同步
        </button>
        <button
          className={`settings-link ${view === "settings" ? "active" : ""}`}
          onClick={() => navigate("settings")}
        >
          <Settings2 size={18} /> 模型与隐私
        </button>
      </aside>

      <main>
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
        {view === "foundation" && (
          <Foundation
            initialSection={foundationSection}
            snapshot={snapshot}
            onUpdate={setSnapshot}
            flash={flash}
          />
        )}
        {view === "ledger" && (
          <PortfolioLedger snapshot={snapshot} flash={flash} />
        )}
        {view === "evidence" && (
          <EvidenceWorkbench navigate={navigate} flash={flash} />
        )}
        {view === "decision" && (
          <DecisionJournal
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
          <ReviewCenter navigate={navigate} flash={flash} />
        )}
        {view === "memory" && <MemoryCenter flash={flash} />}
        {view === "advisor" && model && (
          <Advisor
            model={model}
            navigate={navigate}
            requestedAnalysisId={analysisToOpen}
            clearRequestedAnalysis={() => setAnalysisToOpen(null)}
            onCreateDecisionDraft={(draft) => {
              setDecisionDraft(draft);
              navigate("decision");
            }}
          />
        )}
        {view === "cloud" && (
          <CloudSync flash={flash} onRestore={refreshInvestmentData} />
        )}
        {view === "settings" && model && (
          <ModelSettings model={model} onUpdate={setModel} flash={flash} />
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
