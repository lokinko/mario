import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  Bell,
  BellOff,
  Bot,
  BrainCircuit,
  Check,
  ChevronRight,
  CircleDollarSign,
  Cloud,
  Compass,
  Copy,
  Database,
  Download,
  Edit3,
  Eye,
  FilePenLine,
  History,
  KeyRound,
  LayoutDashboard,
  LoaderCircle,
  LogOut,
  LockKeyhole,
  Plus,
  Save,
  Send,
  Settings2,
  ShieldCheck,
  Sparkles,
  Target,
  Trash2,
  Upload,
  UserRound,
  WalletCards,
} from "lucide-react";
import {
  deleteModelKey,
  deleteHolding,
  deleteGoal,
  getDecisions,
  exportCloudRecoveryKey,
  getCloudConfig,
  getCloudStatus,
  getAnalysis,
  getAnalysisHistory,
  getInvestmentRules,
  getInvestmentRuleHistory,
  getModelConfig,
  getResearchEvidence,
  getReviewReminders,
  getRuleEffectiveness,
  getSnapshot,
  getSystemReviews,
  previewAnalysis,
  pullCloudSync,
  pushCloudSync,
  runAnalysis,
  saveDecision,
  saveCloudConfig,
  saveDecisionReview,
  saveGoal,
  saveHolding,
  saveInvestmentRule,
  saveModelConfig,
  saveProfile,
  saveResearchEvidence,
  saveSystemReview,
  setResearchEvidenceStatus,
  signInCloud,
  signOutCloud,
  signUpCloud,
  testModelConnection,
  updateHolding,
  updateInvestmentRule,
  updateGoal,
  importCloudRecoveryKey,
} from "./api";
import type {
  AnalysisPreview,
  AnalysisClaim,
  AnalysisAction,
  AnalysisEvidenceReference,
  AnalysisRequest,
  AnalysisResult,
  AnalysisHistoryItem,
  AnalysisWorkflowTrace,
  CloudStatus,
  ContextSelection,
  DecisionEntry,
  DecisionRecord,
  DecisionReview,
  DecisionRuleCheck,
  FinancialProfile,
  Goal,
  Holding,
  InvestmentRule,
  InvestmentRuleInput,
  InvestmentRuleRevision,
  MemoryCandidate,
  ModelConfig,
  ResearchEvidence,
  ResearchEvidenceInput,
  Snapshot,
  StructuredAnalysis,
  StoredAnalysis,
  SystemReviewInput,
  SystemReviewRecord,
  ReviewReminderSummary,
  RuleCheckStatus,
  RuleEffectivenessSummary,
} from "./types";
import {
  checkAndSendReviewReminder,
  disableReviewReminders,
  enableReviewReminders,
  isDesktopApp,
} from "./reminders";

type View = "dashboard" | "foundation" | "evidence" | "decision" | "review" | "advisor" | "cloud" | "settings";
const views: View[] = ["dashboard", "foundation", "evidence", "decision", "review", "advisor", "cloud", "settings"];

function initialView(): View {
  const candidate = window.location.hash.replace("#", "") as View;
  return views.includes(candidate) ? candidate : "dashboard";
}

const money = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  maximumFractionDigits: 0,
});

function localDateValue(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

const nav = [
  { id: "dashboard" as const, label: "决策总览", icon: LayoutDashboard },
  { id: "foundation" as const, label: "财务底座", icon: WalletCards },
  { id: "evidence" as const, label: "研究证据", icon: Database },
  { id: "decision" as const, label: "决策日志", icon: FilePenLine },
  { id: "review" as const, label: "复盘与规则", icon: History },
  { id: "advisor" as const, label: "AI 研究室", icon: BrainCircuit },
];

const emptyProfile: FinancialProfile = {
  monthlyIncome: 0,
  monthlyExpense: 0,
  emergencyFund: 0,
  liabilities: 0,
  investableAssets: 0,
  horizonYears: 5,
  maxDrawdownPct: 15,
  riskLevel: "稳健",
};

const emptyHolding: Omit<Holding, "id"> = {
  symbol: "", name: "", assetClass: "基金", marketValue: 0, costBasis: 0, targetPct: 0, currency: "CNY",
};

const emptyGoal: Omit<Goal, "id"> = {
  name: "", targetAmount: 0, currentAmount: 0, monthlyContribution: 0, targetDate: "", priority: "重要",
};

function App() {
  const [view, setView] = useState<View>(initialView);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [model, setModel] = useState<ModelConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [startupError, setStartupError] = useState("");
  const [notice, setNotice] = useState("");
  const [decisionDraft, setDecisionDraft] = useState<DecisionEntry | null>(null);
  const [analysisToOpen, setAnalysisToOpen] = useState<string | null>(null);

  const loadApplication = () => {
    setLoading(true);
    setStartupError("");
    Promise.all([getSnapshot(), getModelConfig()])
      .then(([nextSnapshot, nextModel]) => {
        setSnapshot(nextSnapshot);
        setModel(nextModel);
      })
      .catch((error) => setStartupError(String(error)))
      .finally(() => setLoading(false));
  };

  useEffect(loadApplication, []);

  useEffect(() => {
    if (!loading && !startupError) void checkAndSendReviewReminder().catch(() => undefined);
  }, [loading, startupError]);

  const flash = (message: string) => {
    setNotice(message);
    window.setTimeout(() => setNotice(""), 2400);
  };

  const navigate = (nextView: View) => {
    setView(nextView);
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

  if (startupError || !snapshot || !model) {
    return (
      <div className="center-screen service-error">
        <AlertTriangle size={28} />
        <strong>本地服务尚未就绪</strong>
        <span>客户端没有连接到本次启动的本地服务。你的数据没有丢失。</span>
        <button className="primary" onClick={loadApplication}>重新连接</button>
        <small>{startupError}</small>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><Compass size={20} /></div>
          <div><strong>知衡</strong><span>本地投资决策助手</span></div>
        </div>

        <nav>
          <p className="nav-caption">投资系统</p>
          {nav.map((item) => (
            <button key={item.id} className={view === item.id ? "active" : ""} onClick={() => navigate(item.id)}>
              <item.icon size={18} />{item.label}
            </button>
          ))}
        </nav>

        <div className="sidebar-spacer" />
        <div className="privacy-card">
          <LockKeyhole size={18} />
          <div><strong>本地优先</strong><span>财务档案存储在此设备</span></div>
        </div>
        <button className={`settings-link ${view === "cloud" ? "active" : ""}`} onClick={() => navigate("cloud")}>
          <Cloud size={18} /> 账户与同步
        </button>
        <button className={`settings-link ${view === "settings" ? "active" : ""}`} onClick={() => navigate("settings")}>
          <Settings2 size={18} /> 模型与隐私
        </button>
      </aside>

      <main>
        {notice && <div className="toast"><Check size={16} />{notice}</div>}
        {view === "dashboard" && <Dashboard snapshot={snapshot} model={model} navigate={navigate} />}
        {view === "foundation" && <Foundation snapshot={snapshot} onUpdate={setSnapshot} flash={flash} />}
        {view === "evidence" && <EvidenceWorkbench navigate={navigate} flash={flash} />}
        {view === "decision" && <DecisionJournal flash={flash} seed={decisionDraft} clearSeed={() => setDecisionDraft(null)} onOpenAnalysis={(id) => { setAnalysisToOpen(id); navigate("advisor"); }} />}
        {view === "review" && <ReviewCenter navigate={navigate} flash={flash} />}
        {view === "advisor" && <Advisor model={model} navigate={navigate} requestedAnalysisId={analysisToOpen} clearRequestedAnalysis={() => setAnalysisToOpen(null)} onCreateDecisionDraft={(draft) => { setDecisionDraft(draft); navigate("decision"); }} />}
        {view === "cloud" && <CloudSync flash={flash} />}
        {view === "settings" && <ModelSettings model={model} onUpdate={setModel} flash={flash} />}
      </main>
    </div>
  );
}

function PageHeader({ eyebrow, title, description, action }: { eyebrow: string; title: string; description: string; action?: React.ReactNode }) {
  return (
    <header className="page-header">
      <div><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p>{description}</p></div>
      {action}
    </header>
  );
}

function Dashboard({ snapshot, model, navigate }: { snapshot: Snapshot; model: ModelConfig; navigate: (view: View) => void }) {
  const allocation = useMemo(() => {
    const totals = new Map<string, number>();
    snapshot.holdings.forEach((h) => totals.set(h.assetClass, (totals.get(h.assetClass) ?? 0) + h.marketValue));
    return [...totals.entries()].map(([name, value]) => ({ name, value, pct: snapshot.totalValue ? value / snapshot.totalValue * 100 : 0 }));
  }, [snapshot]);

  const readiness = Math.min(100, Math.round(
    (snapshot.emergencyMonths >= 6 ? 25 : snapshot.emergencyMonths / 6 * 25) +
    (snapshot.holdings.length > 2 ? 25 : snapshot.holdings.length / 3 * 25) +
    (snapshot.profile.horizonYears > 0 ? 25 : 0) +
    (model.hasApiKey ? 25 : 10),
  ));

  return (
    <div className="page">
      <PageHeader
        eyebrow="今天不需要预测市场"
        title="先看目标，再看风险"
        description="这里衡量的是决策质量，而不是鼓励更多交易。"
        action={<button className="primary" onClick={() => navigate("advisor")}><Sparkles size={17} />开始一次分析</button>}
      />

      <section className="metric-grid">
        <article className="metric hero-metric">
          <span>可投资资产</span><strong>{money.format(snapshot.totalValue)}</strong>
          <small>最近更新 · {new Date(snapshot.updatedAt).toLocaleDateString("zh-CN")}</small>
        </article>
        <article className="metric"><span>应急覆盖</span><strong>{snapshot.emergencyMonths.toFixed(1)} <em>个月</em></strong><small className={snapshot.emergencyMonths >= 6 ? "positive" : "warning"}>{snapshot.emergencyMonths >= 6 ? "处于建议区间" : "建议优先补足"}</small></article>
        <article className="metric"><span>最大资产占比</span><strong>{snapshot.concentrationPct.toFixed(1)}%</strong><small>需要结合资产性质判断</small></article>
        <article className="metric"><span>系统准备度</span><strong>{readiness}<em>/100</em></strong><div className="progress"><i style={{ width: `${readiness}%` }} /></div></article>
      </section>

      <section className="two-columns">
        <article className="panel">
          <div className="panel-title"><div><span>组合结构</span><h2>钱现在在哪里</h2></div><button className="text-button" onClick={() => navigate("foundation")}>管理资产 <ChevronRight size={15} /></button></div>
          <div className="allocation">
            <div className="donut" style={{ background: allocationGradient(allocation) }}><div><strong>{allocation.length}</strong><span>类资产</span></div></div>
            <div className="legend">
              {allocation.map((item, index) => <div key={item.name}><i className={`color-${index % 5}`} /><span>{item.name}</span><strong>{item.pct.toFixed(1)}%</strong></div>)}
            </div>
          </div>
        </article>

        <article className="panel">
          <div className="panel-title"><div><span>规则引擎</span><h2>优先处理的风险</h2></div><ShieldCheck size={22} className="muted-icon" /></div>
          <div className="finding-list">
            {snapshot.findings.length === 0 && <div className="empty">完成财务档案后，这里会出现确定性风险检查。</div>}
            {snapshot.findings.slice(0, 3).map((finding, index) => (
              <div className={`finding ${finding.level}`} key={`${finding.title}-${index}`}>
                <AlertTriangle size={17} /><div><strong>{finding.title}</strong><p>{finding.detail}</p><small>{finding.action}</small></div>
              </div>
            ))}
          </div>
        </article>
      </section>

      <section className="planning-grid">
        <article className="panel">
          <div className="panel-title"><div><span>目标可行性</span><h2>计划能否覆盖目标</h2></div><Target size={22} className="muted-icon" /></div>
          {snapshot.plan.goalProjections.length === 0
            ? <div className="empty">添加目标的已投入金额和月度投入后，这里会生成概率情景。</div>
            : <div className="projection-list">{snapshot.plan.goalProjections.map((goal) => (
              <div key={goal.goalId}>
                <div className="projection-head"><strong>{goal.name}</strong><span className={goal.status}>{goalStatus(goal.status)}</span></div>
                <div className="projection-bar"><i style={{ width: `${Math.min(100, goal.estimatedSuccessPct)}%` }} /></div>
                <div className="projection-stats"><span>模拟达成率 <b>{goal.estimatedSuccessPct.toFixed(0)}%</b></span><span>月度缺口 <b>{money.format(goal.monthlyGap)}</b></span><span>剩余 <b>{goal.monthsRemaining} 个月</b></span></div>
              </div>
            ))}</div>}
        </article>
        <article className="panel">
          <div className="panel-title"><div><span>风险预算与再平衡</span><h2>风险有没有超出边界</h2></div><ShieldCheck size={22} className="muted-icon" /></div>
          <div className={`risk-budget ${snapshot.plan.riskStatus}`}><div><span>压力损失估计</span><strong>{snapshot.plan.stressLossPct.toFixed(1)}%</strong></div><ArrowRight size={17} /><div><span>当前风险容量</span><strong>{snapshot.plan.riskCapacityPct.toFixed(1)}%</strong></div><em>{riskStatus(snapshot.plan.riskStatus)}</em></div>
          {snapshot.plan.rebalancing.length > 0
            ? <div className="rebalance-list">{snapshot.plan.rebalancing.slice(0, 4).map((item) => <div key={item.holdingId}><span>{item.name}</span><small>{item.currentPct.toFixed(1)}% → {item.targetPct.toFixed(1)}%</small><strong>{item.direction} {money.format(item.amount)}</strong></div>)}</div>
            : <p className="planning-empty">持仓目标权重合计达到 100%，且偏差超过 3 个百分点时生成再平衡提示。</p>}
          <p className="assumption-note">{snapshot.plan.assumptions}</p>
        </article>
      </section>

      <section className="method-strip">
        <div><p className="eyebrow">知衡决策闭环</p><h2>每一次判断，都留下可复盘的证据</h2></div>
        {["财务底座", "目标配置", "独立研究", "仓位决策", "复盘校准"].map((step, index) => (
          <div className="method-step" key={step}><span>0{index + 1}</span><strong>{step}</strong>{index < 4 && <ArrowRight size={15} />}</div>
        ))}
      </section>
    </div>
  );
}

function goalStatus(status: Snapshot["plan"]["goalProjections"][number]["status"]) {
  return ({ "on-track": "路径较稳", watch: "需要关注", "off-track": "存在缺口", reached: "已经达成", expired: "目标到期" })[status];
}

function riskStatus(status: Snapshot["plan"]["riskStatus"]) {
  return ({ within: "边界内", near: "接近上限", over: "超出边界", insufficient: "等待持仓" })[status];
}

function allocationGradient(allocation: { pct: number }[]) {
  const colors = ["#cf5c3b", "#23443b", "#c99a45", "#80948f", "#774936"];
  let cursor = 0;
  const stops = allocation.map((item, index) => {
    const start = cursor;
    cursor += item.pct;
    return `${colors[index % colors.length]} ${start}% ${cursor}%`;
  });
  return `conic-gradient(${stops.join(",")})`;
}

function Foundation({ snapshot, onUpdate, flash }: { snapshot: Snapshot; onUpdate: (s: Snapshot) => void; flash: (s: string) => void }) {
  const [profile, setProfile] = useState(snapshot.profile ?? emptyProfile);
  const [holding, setHolding] = useState<Omit<Holding, "id">>(emptyHolding);
  const [editingHoldingId, setEditingHoldingId] = useState<string | null>(null);
  const [goal, setGoal] = useState<Omit<Goal, "id">>(emptyGoal);
  const [editingGoalId, setEditingGoalId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const updateNumber = (key: keyof FinancialProfile, value: string) => setProfile({ ...profile, [key]: Number(value) });

  const persistProfile = async () => {
    setSaving(true);
    try { onUpdate(await saveProfile(profile)); flash("财务档案已保存在本机"); } finally { setSaving(false); }
  };

  const persistHolding = async () => {
    if (!holding.name || holding.marketValue <= 0) return;
    setSaving(true);
    try {
      const next = editingHoldingId
        ? await updateHolding(editingHoldingId, holding)
        : await saveHolding(holding);
      onUpdate(next);
      setHolding(emptyHolding);
      setEditingHoldingId(null);
      flash(editingHoldingId ? "资产信息已更新" : "资产已加入组合");
    } finally { setSaving(false); }
  };

  const editHolding = (item: Holding) => {
    const { id, ...values } = item;
    setHolding(values);
    setEditingHoldingId(id);
  };

  const removeHolding = async (item: Holding) => {
    if (!window.confirm(`确认删除“${item.name}”？相关决策日志不会被删除。`)) return;
    setSaving(true);
    try {
      onUpdate(await deleteHolding(item.id));
      if (editingHoldingId === item.id) { setEditingHoldingId(null); setHolding(emptyHolding); }
      flash("资产已从组合删除");
    } finally { setSaving(false); }
  };

  const persistGoal = async () => {
    if (!goal.name || goal.targetAmount <= 0 || !goal.targetDate) return;
    setSaving(true);
    try {
      const next = editingGoalId ? await updateGoal(editingGoalId, goal) : await saveGoal(goal);
      onUpdate(next);
      setGoal(emptyGoal);
      setEditingGoalId(null);
      flash(editingGoalId ? "投资目标已更新" : "投资目标已保存");
    } finally { setSaving(false); }
  };

  const editGoal = (item: Goal) => {
    const { id, ...values } = item;
    setGoal(values);
    setEditingGoalId(id);
  };

  const removeGoal = async (item: Goal) => {
    if (!window.confirm(`确认删除目标“${item.name}”？`)) return;
    setSaving(true);
    try {
      onUpdate(await deleteGoal(item.id));
      if (editingGoalId === item.id) { setEditingGoalId(null); setGoal(emptyGoal); }
      flash("投资目标已删除");
    } finally { setSaving(false); }
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="方法论 · 第一层" title="建立财务底座" description="先确定哪些钱能承担风险，再讨论收益。数据仅保存在本地数据库。" />
      <section className="panel form-panel">
        <div className="panel-title"><div><span>个人资产负债表</span><h2>现金流与风险边界</h2></div><Database size={21} className="muted-icon" /></div>
        <div className="form-grid">
          <NumberField label="月收入" value={profile.monthlyIncome} onChange={(v) => updateNumber("monthlyIncome", v)} prefix="¥" />
          <NumberField label="月支出" value={profile.monthlyExpense} onChange={(v) => updateNumber("monthlyExpense", v)} prefix="¥" />
          <NumberField label="应急资金" value={profile.emergencyFund} onChange={(v) => updateNumber("emergencyFund", v)} prefix="¥" />
          <NumberField label="负债余额" value={profile.liabilities} onChange={(v) => updateNumber("liabilities", v)} prefix="¥" />
          <NumberField label="可投资资产" value={profile.investableAssets} onChange={(v) => updateNumber("investableAssets", v)} prefix="¥" />
          <NumberField label="投资期限" value={profile.horizonYears} onChange={(v) => updateNumber("horizonYears", v)} suffix="年" />
          <NumberField label="可承受最大回撤" value={profile.maxDrawdownPct} onChange={(v) => updateNumber("maxDrawdownPct", v)} suffix="%" />
          <label><span>风险倾向</span><select value={profile.riskLevel} onChange={(e) => setProfile({ ...profile, riskLevel: e.target.value as FinancialProfile["riskLevel"] })}><option>保守</option><option>稳健</option><option>均衡</option><option>进取</option></select></label>
        </div>
        <div className="form-actions"><p><ShieldCheck size={16} />系统不会把“心理上敢亏”误认为真实风险承受能力。</p><button className="primary" onClick={persistProfile} disabled={saving}><Save size={16} />保存并检查</button></div>
      </section>

      <section className="panel form-panel">
        <div className="panel-title"><div><span>目标账户</span><h2>{editingGoalId ? "修改目标计划" : "给资金一个明确任务"}</h2></div><Target size={21} className="muted-icon" /></div>
        {snapshot.goals.length > 0 && <div className="goal-list">{snapshot.goals.map((item) => {
          const projection = snapshot.plan.goalProjections.find((value) => value.goalId === item.id);
          return <div className={editingGoalId === item.id ? "editing" : ""} key={item.id}><span>{item.priority}</span><strong>{item.name}<small>已投入 {money.format(item.currentAmount)} · 每月 {money.format(item.monthlyContribution)}</small></strong><em>{projection ? `模拟达成 ${projection.estimatedSuccessPct.toFixed(0)}%` : item.targetDate}</em><span className="row-actions"><button aria-label="编辑目标" onClick={() => editGoal(item)}><Edit3 size={14} /></button><button aria-label="删除目标" onClick={() => removeGoal(item)}><Trash2 size={14} /></button></span></div>;
        })}</div>}
        <div className="form-grid compact-grid">
          <label><span>目标名称</span><input value={goal.name} onChange={(e) => setGoal({ ...goal, name: e.target.value })} placeholder="例如：长期养老账户" /></label>
          <NumberField label="目标金额" value={goal.targetAmount} onChange={(v) => setGoal({ ...goal, targetAmount: Number(v) })} prefix="¥" />
          <NumberField label="已经投入" value={goal.currentAmount} onChange={(v) => setGoal({ ...goal, currentAmount: Number(v) })} prefix="¥" />
          <NumberField label="计划每月投入" value={goal.monthlyContribution} onChange={(v) => setGoal({ ...goal, monthlyContribution: Number(v) })} prefix="¥" />
          <label><span>目标日期</span><input type="date" value={goal.targetDate} onChange={(e) => setGoal({ ...goal, targetDate: e.target.value })} /></label>
          <label><span>目标优先级</span><select value={goal.priority} onChange={(e) => setGoal({ ...goal, priority: e.target.value as Goal["priority"] })}><option>刚性</option><option>重要</option><option>弹性</option></select></label>
        </div>
        <div className="form-actions">{editingGoalId ? <button className="text-button" onClick={() => { setEditingGoalId(null); setGoal(emptyGoal); }}>取消修改</button> : <p>目标决定期限，期限决定可以承担的波动。</p>}<button className="secondary" onClick={persistGoal} disabled={saving || !goal.name}>{editingGoalId ? <Save size={16} /> : <Plus size={16} />}{editingGoalId ? "保存目标" : "添加目标"}</button></div>
      </section>

      <section className="panel form-panel">
        <div className="panel-title"><div><span>组合输入</span><h2>{editingHoldingId ? "修改资产" : "管理资产组合"}</h2></div><CircleDollarSign size={21} className="muted-icon" /></div>
        {snapshot.holdings.length > 0 && <div className="holding-list">
          <div className="holding-head"><span>资产</span><span>类别</span><span>市值</span><span>目标权重</span><span>账面变化</span><span /></div>
          {snapshot.holdings.map((item) => {
            const pnlPct = item.costBasis > 0 ? (item.marketValue - item.costBasis) / item.costBasis * 100 : 0;
            return <div className={editingHoldingId === item.id ? "editing" : ""} key={item.id}>
              <strong>{item.name}<small>{item.symbol || "未填写代码"}</small></strong>
              <span>{item.assetClass}</span><span>{money.format(item.marketValue)}</span><span>{item.targetPct ? `${item.targetPct.toFixed(1)}%` : "未设置"}</span>
              <span className={pnlPct >= 0 ? "gain" : "loss"}>{pnlPct >= 0 ? "+" : ""}{pnlPct.toFixed(1)}%</span>
              <span className="row-actions"><button aria-label="编辑资产" onClick={() => editHolding(item)}><Edit3 size={14} /></button><button aria-label="删除资产" onClick={() => removeHolding(item)}><Trash2 size={14} /></button></span>
            </div>;
          })}
        </div>}
        <div className="form-grid compact-grid">
          <label><span>资产名称</span><input value={holding.name} onChange={(e) => setHolding({ ...holding, name: e.target.value })} placeholder="例如：宽基指数基金" /></label>
          <label><span>代码（可选）</span><input value={holding.symbol} onChange={(e) => setHolding({ ...holding, symbol: e.target.value })} placeholder="例如：000300" /></label>
          <label><span>资产类别</span><select value={holding.assetClass} onChange={(e) => setHolding({ ...holding, assetClass: e.target.value as Holding["assetClass"] })}>{["现金", "债券", "股票", "基金", "黄金", "其他"].map((v) => <option key={v}>{v}</option>)}</select></label>
          <NumberField label="当前市值" value={holding.marketValue} onChange={(v) => setHolding({ ...holding, marketValue: Number(v) })} prefix="¥" />
          <NumberField label="累计成本" value={holding.costBasis} onChange={(v) => setHolding({ ...holding, costBasis: Number(v) })} prefix="¥" />
          <NumberField label="目标权重" value={holding.targetPct} onChange={(v) => setHolding({ ...holding, targetPct: Number(v) })} suffix="%" />
        </div>
        <div className="form-actions">
          {editingHoldingId ? <button className="text-button" onClick={() => { setEditingHoldingId(null); setHolding(emptyHolding); }}>取消修改</button> : <span />}
          <button className="secondary" onClick={persistHolding} disabled={saving || !holding.name}>{editingHoldingId ? <Save size={16} /> : <Plus size={16} />}{editingHoldingId ? "保存修改" : "加入组合"}</button>
        </div>
      </section>
    </div>
  );
}

function NumberField({ label, value, onChange, prefix, suffix }: { label: string; value: number; onChange: (v: string) => void; prefix?: string; suffix?: string }) {
  return <label><span>{label}</span><div className="input-affix">{prefix && <i>{prefix}</i>}<input type="number" value={value || ""} onChange={(e) => onChange(e.target.value)} />{suffix && <i>{suffix}</i>}</div></label>;
}

function emptyEvidence(): ResearchEvidenceInput {
  return {
    assetName: "",
    title: "",
    publisher: "",
    sourceUrl: "",
    sourceTier: "一手来源",
    evidenceType: "公司披露",
    stance: "背景",
    asOfDate: localDateValue(new Date()),
    claim: "",
    notes: "",
  };
}

function sourceHost(value: string) {
  try { return new URL(value).hostname; }
  catch { return value; }
}

function EvidenceWorkbench({ navigate, flash }: { navigate: (v: View) => void; flash: (s: string) => void }) {
  const [draft, setDraft] = useState<ResearchEvidenceInput>(emptyEvidence);
  const [items, setItems] = useState<ResearchEvidence[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    try { setItems(await getResearchEvidence()); setError(""); }
    catch (nextError) { setError(String(nextError)); }
  };

  useEffect(() => { void refresh(); }, []);

  const active = items.filter((item) => item.active);
  const primary = active.filter((item) => item.sourceTier === "一手来源");
  const counter = active.filter((item) => item.stance === "反驳");
  const oneYearAgo = new Date();
  oneYearAgo.setFullYear(oneYearAgo.getFullYear() - 1);
  const aging = active.filter((item) => new Date(`${item.asOfDate}T00:00:00`) < oneYearAgo);

  const persist = async () => {
    if (!draft.assetName || !draft.title || !draft.publisher || !draft.sourceUrl || !draft.asOfDate || !draft.claim) return;
    setSaving(true); setError("");
    try {
      await saveResearchEvidence(draft);
      setDraft(emptyEvidence());
      await refresh();
      flash("研究证据已保存在本机，原始内容将保持不变");
    } catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  const toggleStatus = async (item: ResearchEvidence) => {
    setSaving(true); setError("");
    try {
      await setResearchEvidenceStatus(item.id, !item.active);
      await refresh();
      flash(item.active ? "证据已归档，不再进入 AI 检索" : "证据已恢复为有效状态");
    } catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  const startEvidenceAnalysis = () => {
    const assets = [...new Set(active.map((item) => item.assetName))].join("、");
    window.sessionStorage.setItem(
      "compass.advisorQuestion",
      `请基于我保存的带来源研究证据，审查${assets || "当前组合"}的投资假设：区分一手事实、二手解释与未知项，优先寻找反方证据，只引用载荷中实际存在的 HTTPS 来源，并给出下一步需要补齐的证据。`,
    );
    navigate("advisor");
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="方法论 · 研究层" title="观点之前，先建立证据" description="保存事实出处、资料日期与反方证据；AI 只能引用你确认发送的记录。" action={<button className="primary" onClick={startEvidenceAnalysis} disabled={!active.length}><Sparkles size={16} />用证据开始分析</button>} />
      <section className="review-metrics">
        <article><span>有效证据</span><strong>{active.length}</strong><small>{items.length - active.length} 条已归档</small></article>
        <article><span>一手来源</span><strong>{primary.length}</strong><small>披露、监管或原始数据</small></article>
        <article><span>反方证据</span><strong>{counter.length}</strong><small>避免只收集支持材料</small></article>
        <article><span>超过一年</span><strong className={aging.length ? "warning-text" : ""}>{aging.length}</strong><small>过期不等于错误，但需要复核</small></article>
      </section>

      <section className="panel evidence-form">
        <div className="panel-title"><div><span>证据账本</span><h2>记录一条可追溯事实</h2></div><Database size={21} className="muted-icon" /></div>
        <div className="evidence-entry-layout">
          <div className="form-grid">
            <label><span>关联资产或主题</span><input maxLength={120} value={draft.assetName} onChange={(e) => setDraft({ ...draft, assetName: e.target.value })} placeholder="例如：全球指数、黄金、某家公司" /></label>
            <label><span>资料标题</span><input maxLength={300} value={draft.title} onChange={(e) => setDraft({ ...draft, title: e.target.value })} placeholder="使用来源页面的准确标题" /></label>
            <label><span>发布方</span><input maxLength={200} value={draft.publisher} onChange={(e) => setDraft({ ...draft, publisher: e.target.value })} placeholder="公司、监管机构或研究机构" /></label>
            <label><span>HTTPS 来源链接</span><input type="url" maxLength={2048} value={draft.sourceUrl} onChange={(e) => setDraft({ ...draft, sourceUrl: e.target.value })} placeholder="https://…（不要包含访问令牌）" /></label>
            <label><span>来源层级</span><select value={draft.sourceTier} onChange={(e) => setDraft({ ...draft, sourceTier: e.target.value as ResearchEvidenceInput["sourceTier"] })}><option>一手来源</option><option>二手研究</option><option>媒体报道</option></select></label>
            <label><span>证据类型</span><select value={draft.evidenceType} onChange={(e) => setDraft({ ...draft, evidenceType: e.target.value as ResearchEvidenceInput["evidenceType"] })}>{["公司披露", "监管文件", "数据发布", "研究报告", "新闻", "其他"].map((value) => <option key={value}>{value}</option>)}</select></label>
            <label><span>与当前假设的关系</span><select value={draft.stance} onChange={(e) => setDraft({ ...draft, stance: e.target.value as ResearchEvidenceInput["stance"] })}><option>支持</option><option>反驳</option><option>背景</option></select></label>
            <label><span>资料日期</span><input type="date" max={localDateValue(new Date())} value={draft.asOfDate} onChange={(e) => setDraft({ ...draft, asOfDate: e.target.value })} /></label>
            <label className="span-2"><span>这条来源实际支持什么事实？</span><textarea maxLength={4000} value={draft.claim} onChange={(e) => setDraft({ ...draft, claim: e.target.value })} placeholder="只记录来源能够直接支持的内容，不写买卖结论。" /></label>
            <label className="span-2"><span>限制与待核实项（可选）</span><textarea maxLength={4000} value={draft.notes} onChange={(e) => setDraft({ ...draft, notes: e.target.value })} placeholder="口径差异、样本限制、尚未核验的解释。" /></label>
          </div>
          <aside className="evidence-guide"><strong>来源不是结论</strong><p>“一手来源”表示离原始事实更近，不代表内容完整或投资判断正确。</p><ol><li>优先保存公司披露、监管文件和原始数据。</li><li>支持材料与反方材料分开记录。</li><li>错误记录应归档并重新建立，不覆盖旧证据。</li><li>知衡当前不自动抓取或核验链接内容。</li></ol></aside>
        </div>
        {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}
        <div className="form-actions"><p>保存后内容不可编辑；归档是可恢复操作。</p><button className="primary" onClick={persist} disabled={saving || !draft.assetName || !draft.title || !draft.publisher || !draft.sourceUrl || !draft.asOfDate || !draft.claim}><Save size={16} />保存证据</button></div>
      </section>

      <section className="panel evidence-library">
        <div className="panel-title"><div><span>本地证据库</span><h2>检查来源结构，而不是累计观点数量</h2></div><span className="history-count">{items.length} 条</span></div>
        {items.length === 0 && <div className="empty">还没有研究证据。先从一条可以打开、可以标注日期的一手来源开始。</div>}
        <div className="evidence-list">{items.map((item) => <article key={item.id} className={item.active ? "" : "inactive"}>
          <div className="evidence-head"><div><span className={`stance-${item.stance}`}>{item.stance}</span><strong>{item.assetName}</strong><em>{item.sourceTier}</em></div><small>{item.asOfDate}</small></div>
          <h3>{item.title}</h3><p>{item.claim}</p>{item.notes && <small className="evidence-notes">限制：{item.notes}</small>}
          <footer><a href={item.sourceUrl} target="_blank" rel="noreferrer">{item.publisher} · {sourceHost(item.sourceUrl)}</a><button className="text-button" disabled={saving} onClick={() => toggleStatus(item)}>{item.active ? "归档" : "恢复"}</button></footer>
        </article>)}</div>
      </section>
    </div>
  );
}

const emptyDecision: DecisionEntry = {
  assetName: "", thesis: "", counterThesis: "", expectedReturnPct: 0, downsidePct: 0,
  confidencePct: 50, positionPct: 0, invalidation: "", reviewDate: "", ruleChecks: [],
};

function checksForRules(rules: InvestmentRule[]): DecisionRuleCheck[] {
  return rules.filter((rule) => rule.active).map((rule) => ({
    ruleId: rule.id,
    ruleRevision: rule.revision,
    category: rule.category,
    statement: rule.statement,
    trigger: rule.trigger,
    status: "待确认",
    note: "",
  }));
}

function DecisionJournal({ flash, seed, clearSeed, onOpenAnalysis }: { flash: (s: string) => void; seed: DecisionEntry | null; clearSeed: () => void; onOpenAnalysis: (id: string) => void }) {
  const [entry, setEntry] = useState({ ...emptyDecision });
  const [records, setRecords] = useState<DecisionRecord[]>([]);
  const [rules, setRules] = useState<InvestmentRule[]>([]);
  const [reviewingId, setReviewingId] = useState<string | null>(null);
  const [review, setReview] = useState<DecisionReview>({ outcomeSummary: "", thesisStatus: "尚不明确", processRating: 3, lessons: "" });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const expectedValue = entry.confidencePct / 100 * entry.expectedReturnPct - (1 - entry.confidencePct / 100) * Math.abs(entry.downsidePct);

  const refresh = async () => {
    try {
      const [nextRecords, nextRules] = await Promise.all([getDecisions(), getInvestmentRules()]);
      setRecords(nextRecords);
      setRules(nextRules);
      setEntry((current) => current.ruleChecks?.length ? current : { ...current, ruleChecks: checksForRules(nextRules) });
      setError("");
    }
    catch (nextError) { setError(String(nextError)); }
  };

  useEffect(() => { void refresh(); }, []);
  useEffect(() => { if (seed) setEntry({ ...seed, ruleChecks: checksForRules(rules) }); }, [seed, rules]);

  const activeRules = rules.filter((rule) => rule.active);
  const incompleteRuleChecks = activeRules.length !== (entry.ruleChecks?.length ?? 0)
    || (entry.ruleChecks ?? []).some((check) => check.status === "待确认" || (check.status === "偏离" && !check.note.trim()));

  const updateRuleCheck = (ruleId: string, update: Partial<DecisionRuleCheck>) => {
    setEntry({
      ...entry,
      ruleChecks: (entry.ruleChecks ?? []).map((check) => check.ruleId === ruleId ? { ...check, ...update } : check),
    });
  };

  const reviewed = records.filter((item) => item.review);
  const calibration = useMemo(() => {
    const measurable = reviewed.filter((item) => item.review?.thesisStatus !== "尚不明确");
    if (!measurable.length) return null;
    const brier = measurable.reduce((sum, item) => {
      const outcome = item.review?.thesisStatus === "成立" ? 1 : item.review?.thesisStatus === "失效" ? 0 : 0.5;
      return sum + Math.pow(item.confidencePct / 100 - outcome, 2);
    }, 0) / measurable.length;
    return Math.max(0, Math.round((1 - brier) * 100));
  }, [reviewed]);

  const processAverage = reviewed.length
    ? reviewed.reduce((sum, item) => sum + (item.review?.processRating ?? 0), 0) / reviewed.length
    : null;

  const persist = async () => {
    if (!entry.assetName || !entry.thesis || !entry.counterThesis || !entry.invalidation || !entry.reviewDate || incompleteRuleChecks) return;
    setSaving(true);
    try {
      await saveDecision(entry);
      flash("决策快照已冻结，可用于未来复盘");
      setEntry({ ...emptyDecision, ruleChecks: checksForRules(rules) });
      clearSeed();
      await refresh();
    } catch (nextError) { setError(String(nextError)); }
    finally { setSaving(false); }
  };

  const beginReview = (record: DecisionRecord) => {
    setReviewingId(record.id);
    setReview(record.review ?? { outcomeSummary: "", thesisStatus: "尚不明确", processRating: 3, lessons: "" });
  };

  const persistReview = async () => {
    if (!reviewingId || !review.outcomeSummary || !review.lessons) return;
    setSaving(true);
    try {
      await saveDecisionReview(reviewingId, review);
      await refresh();
      setReviewingId(null);
      flash("复盘已保存，判断校准数据已更新");
    } finally { setSaving(false); }
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="方法论 · 决策层" title="先写下来，再按下买入" description="记录当时真正知道的事，避免用事后结果改写记忆。" />
      <section className="review-metrics">
        <article><span>决策记录</span><strong>{records.length}</strong><small>原始判断不可被复盘覆盖</small></article>
        <article><span>已完成复盘</span><strong>{reviewed.length}</strong><small>{records.length ? `${Math.round(reviewed.length / records.length * 100)}% 完成率` : "等待第一条记录"}</small></article>
        <article><span>简化校准分</span><strong>{calibration === null ? "—" : `${calibration}`}</strong><small>{calibration === null ? "至少需要一条明确结果" : `${reviewed.length} 个样本，仅作训练`}</small></article>
        <article><span>过程评分</span><strong>{processAverage === null ? "—" : processAverage.toFixed(1)}</strong><small>独立于实际盈亏</small></article>
      </section>
      <section className="panel form-panel decision-card">
        <div className="panel-title"><div><span>投资决策卡</span><h2>把观点变成可证伪的假设</h2></div><FilePenLine size={21} className="muted-icon" /></div>
        {entry.sourceAnalysisId && <div className="decision-draft-notice"><BrainCircuit size={18} /><div><strong>来自 AI 分析的待确认草稿</strong><p>只复制了文字线索。投资对象、收益、损失、置信度、仓位和复盘日期必须由你判断；点击“冻结”前不会形成决策。</p></div><button className="text-button" onClick={() => { setEntry({ ...emptyDecision }); clearSeed(); }}>放弃草稿</button></div>}
        <div className="form-grid">
          <label className="span-2"><span>投资对象</span><input value={entry.assetName} onChange={(e) => setEntry({ ...entry, assetName: e.target.value })} placeholder="你真正购买的是什么？" /></label>
          <label className="span-2"><span>核心逻辑</span><textarea value={entry.thesis} onChange={(e) => setEntry({ ...entry, thesis: e.target.value })} placeholder="收益从哪里来？市场可能错在哪里？" /></label>
          <label className="span-2"><span>最强反方观点</span><textarea value={entry.counterThesis} onChange={(e) => setEntry({ ...entry, counterThesis: e.target.value })} placeholder="站在反方立场，什么最可能让这笔投资失败？" /></label>
          <NumberField label="上行情景收益" value={entry.expectedReturnPct} onChange={(v) => setEntry({ ...entry, expectedReturnPct: Number(v) })} suffix="%" />
          <NumberField label="下行情景损失" value={entry.downsidePct} onChange={(v) => setEntry({ ...entry, downsidePct: Number(v) })} suffix="%" />
          <NumberField label="判断置信度" value={entry.confidencePct} onChange={(v) => setEntry({ ...entry, confidencePct: Number(v) })} suffix="%" />
          <NumberField label="计划仓位" value={entry.positionPct} onChange={(v) => setEntry({ ...entry, positionPct: Number(v) })} suffix="%" />
          <label className="span-2"><span>证伪条件</span><textarea value={entry.invalidation} onChange={(e) => setEntry({ ...entry, invalidation: e.target.value })} placeholder="出现什么证据时，你会承认原始判断已经失效？" /></label>
          <label><span>计划复盘日</span><input type="date" value={entry.reviewDate} onChange={(e) => setEntry({ ...entry, reviewDate: e.target.value })} /></label>
          <div className={`ev-card ${expectedValue >= 0 ? "positive-bg" : "negative-bg"}`}><span>粗略概率加权结果</span><strong>{expectedValue > 0 ? "+" : ""}{expectedValue.toFixed(1)}%</strong><small>仅作思考校准，不代表预测</small></div>
        </div>
        <div className="rule-check-workbench">
          <div className="rule-check-title"><div><span>决策前规则检查</span><strong>逐条确认，而不是事后声称自己遵守了纪律</strong></div><small>{activeRules.length ? `${activeRules.length} 条当前有效规则` : "当前没有有效规则"}</small></div>
          {activeRules.length === 0 && <div className="empty">还没有需要确认的个人规则。可在“复盘与规则”中把经验沉淀成可执行约束。</div>}
          <div className="rule-check-list">{(entry.ruleChecks ?? []).map((check) => <article key={check.ruleId} className={`status-${check.status}`}>
            <div><span>{check.category} · v{check.ruleRevision}</span><strong>{check.statement}</strong><small>触发：{check.trigger}</small></div>
            <label><span>本次判断</span><select value={check.status} onChange={(event) => updateRuleCheck(check.ruleId, { status: event.target.value as RuleCheckStatus, note: event.target.value === "偏离" ? check.note : "" })}><option>待确认</option><option>遵守</option><option>偏离</option><option>不适用</option></select></label>
            {check.status === "偏离" && <label className="rule-deviation-note"><span>偏离原因（必填）</span><input maxLength={1000} value={check.note} onChange={(event) => updateRuleCheck(check.ruleId, { note: event.target.value })} placeholder="为什么仍决定偏离？需要什么证据纠正？" /></label>}
          </article>)}</div>
        </div>
        <div className="form-actions"><p>{incompleteRuleChecks ? "请先完成所有有效规则的逐条确认" : "必填：投资对象、正反逻辑、证伪条件与复盘日期"}</p><button className="primary" onClick={persist} disabled={saving || !entry.assetName || !entry.thesis || !entry.counterThesis || !entry.invalidation || !entry.reviewDate || incompleteRuleChecks}><Save size={16} />冻结决策快照</button></div>
      </section>

      <section className="panel decision-history">
        <div className="panel-title"><div><span>历史证据</span><h2>按原始假设复盘，而不是看盈亏讲故事</h2></div><span className="history-count">{records.length} 条</span></div>
        {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}
        {!error && records.length === 0 && <div className="empty">还没有决策记录。先冻结一张决策卡，未来才有可复盘的证据。</div>}
        <div className="decision-list">
          {records.map((record) => {
            const due = Boolean(record.reviewDate && record.reviewDate <= localDateValue(new Date()) && !record.review);
            return <article key={record.id} className={reviewingId === record.id ? "reviewing" : ""}>
              <div className="decision-summary">
                <div className="decision-name"><span className={record.review ? "reviewed" : due ? "due" : "planned"}>{record.review ? "已复盘" : due ? "待复盘" : "观察中"}</span><strong>{record.assetName}</strong><small>{new Date(record.createdAt).toLocaleDateString("zh-CN")}</small></div>
                <div><span>置信度</span><strong>{record.confidencePct}%</strong></div>
                <div><span>计划仓位</span><strong>{record.positionPct}%</strong></div>
                <div><span>复盘日</span><strong>{record.reviewDate || "未设定"}</strong></div>
                <button className="secondary" onClick={() => beginReview(record)}>{record.review ? "更新复盘" : "开始复盘"}</button>
              </div>
              <div className="decision-thesis"><p><b>原始逻辑</b>{record.thesis}</p><p><b>证伪条件</b>{record.invalidation}</p>{record.sourceAnalysisId && <button className="decision-provenance" onClick={() => onOpenAnalysis(record.sourceAnalysisId!)}><BrainCircuit size={12} />源自 AI 分析 · 行动 {(record.sourceActionIndex ?? 0) + 1} · 打开原记录</button>}</div>
              {record.ruleChecks?.length > 0 && <div className="decision-rule-snapshot">{record.ruleChecks.map((check) => <div key={check.ruleId}><span className={`rule-check-status status-${check.status}`}>{check.status}</span><strong>{check.statement}</strong><small>v{check.ruleRevision}{check.note ? ` · ${check.note}` : ""}</small></div>)}</div>}
              {record.review && reviewingId !== record.id && <div className="review-result"><span>{record.review.thesisStatus}</span><p>{record.review.outcomeSummary}</p><strong>过程 {record.review.processRating}/5</strong>{record.review.actualReturnPct !== undefined && <em className={record.review.actualReturnPct >= 0 ? "gain" : "loss"}>{record.review.actualReturnPct >= 0 ? "+" : ""}{record.review.actualReturnPct}%</em>}</div>}
              {reviewingId === record.id && <div className="review-form">
                <label><span>原始逻辑结果</span><select value={review.thesisStatus} onChange={(e) => setReview({ ...review, thesisStatus: e.target.value as DecisionReview["thesisStatus"] })}><option>成立</option><option>部分成立</option><option>失效</option><option>尚不明确</option></select></label>
                <label><span>实际收益（可选）</span><div className="input-affix"><input type="number" value={review.actualReturnPct ?? ""} onChange={(e) => setReview({ ...review, actualReturnPct: e.target.value === "" ? undefined : Number(e.target.value) })} /><i>%</i></div></label>
                <label><span>决策过程评分</span><select value={review.processRating} onChange={(e) => setReview({ ...review, processRating: Number(e.target.value) })}>{[1, 2, 3, 4, 5].map((value) => <option key={value} value={value}>{value} / 5</option>)}</select></label>
                <label className="span-3"><span>实际发生了什么？</span><textarea value={review.outcomeSummary} onChange={(e) => setReview({ ...review, outcomeSummary: e.target.value })} placeholder="只记录事实，区分价格结果与逻辑变化。" /></label>
                <label className="span-3"><span>如何修正未来决策？</span><textarea value={review.lessons} onChange={(e) => setReview({ ...review, lessons: e.target.value })} placeholder="保留、修改或删除哪条规则？" /></label>
                <div className="span-3 review-actions"><button className="text-button" onClick={() => setReviewingId(null)}>取消</button><button className="primary" disabled={saving || !review.outcomeSummary || !review.lessons} onClick={persistReview}><Save size={15} />保存复盘</button></div>
              </div>}
            </article>;
          })}
        </div>
      </section>
    </div>
  );
}

const emptyRule: InvestmentRuleInput = {
  category: "风险",
  statement: "",
  trigger: "",
  rationale: "",
  active: true,
};

function defaultSystemReview(): SystemReviewInput {
  const now = new Date();
  const quarter = Math.floor(now.getMonth() / 3) + 1;
  const next = new Date(now);
  next.setMonth(next.getMonth() + 3);
  return {
    periodLabel: `${now.getFullYear()} Q${quarter}`,
    adherenceScore: 3,
    processSummary: "",
    ruleViolations: "",
    lessons: "",
    nextActions: "",
    nextReviewDate: localDateValue(next),
  };
}

function ruleInput(rule: InvestmentRule): InvestmentRuleInput {
  return {
    category: rule.category,
    statement: rule.statement,
    trigger: rule.trigger,
    rationale: rule.rationale,
    active: rule.active,
    sourceReviewId: rule.sourceReviewId,
  };
}

function ReviewCenter({ navigate, flash }: { navigate: (v: View) => void; flash: (s: string) => void }) {
  const [decisions, setDecisions] = useState<DecisionRecord[]>([]);
  const [rules, setRules] = useState<InvestmentRule[]>([]);
  const [reviews, setReviews] = useState<SystemReviewRecord[]>([]);
  const [review, setReview] = useState<SystemReviewInput>(defaultSystemReview);
  const [rule, setRule] = useState<InvestmentRuleInput>(emptyRule);
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [expandedRuleId, setExpandedRuleId] = useState<string | null>(null);
  const [ruleHistories, setRuleHistories] = useState<Record<string, InvestmentRuleRevision[]>>({});
  const [saving, setSaving] = useState(false);
  const [reminder, setReminder] = useState<ReviewReminderSummary | null>(null);
  const [effectiveness, setEffectiveness] = useState<RuleEffectivenessSummary | null>(null);
  const [reminderSaving, setReminderSaving] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    try {
      const [nextDecisions, nextRules, nextReviews, nextReminder, nextEffectiveness] = await Promise.all([
        getDecisions(), getInvestmentRules(), getSystemReviews(), getReviewReminders(), getRuleEffectiveness(),
      ]);
      setDecisions(nextDecisions);
      setRules(nextRules);
      setReviews(nextReviews);
      setReminder(nextReminder);
      setEffectiveness(nextEffectiveness);
      setError("");
    } catch (nextError) { setError(String(nextError)); }
  };

  useEffect(() => { void refresh(); }, []);

  const today = localDateValue(new Date());
  const dueDecisions = decisions.filter((item) => !item.review && item.reviewDate && item.reviewDate <= today);
  const activeRules = rules.filter((item) => item.active);
  const averageAdherence = reviews.length
    ? reviews.reduce((sum, item) => sum + item.adherenceScore, 0) / reviews.length
    : null;
  const latestReview = reviews[0];
  const periodicReviewDue = !latestReview || latestReview.nextReviewDate <= today;

  const persistSystemReview = async () => {
    if (!review.periodLabel || !review.processSummary || !review.lessons || !review.nextActions || !review.nextReviewDate) return;
    setSaving(true); setError("");
    try {
      await saveSystemReview(review);
      setReview(defaultSystemReview());
      await refresh();
      flash("周期复盘已冻结，并保留当时的组合与方法快照");
    } catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  const persistRule = async () => {
    if (!rule.statement || !rule.trigger || !rule.rationale) return;
    setSaving(true); setError("");
    try {
      if (editingRuleId) await updateInvestmentRule(editingRuleId, rule);
      else await saveInvestmentRule(rule);
      if (editingRuleId) {
        setRuleHistories((current) => {
          const next = { ...current };
          delete next[editingRuleId];
          return next;
        });
        setExpandedRuleId(null);
      }
      setRule(emptyRule);
      setEditingRuleId(null);
      await refresh();
      flash(editingRuleId ? "规则已产生新版本，旧版本仍保留" : "个人投资规则已建立");
    } catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  const toggleRule = async (item: InvestmentRule) => {
    setSaving(true); setError("");
    try {
      await updateInvestmentRule(item.id, { ...ruleInput(item), active: !item.active });
      setRuleHistories((current) => {
        const next = { ...current };
        delete next[item.id];
        return next;
      });
      setExpandedRuleId(null);
      await refresh();
      flash(item.active ? "规则已停用，历史版本仍保留" : "规则已重新启用");
    } catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  const toggleRuleHistory = async (item: InvestmentRule) => {
    if (expandedRuleId === item.id) { setExpandedRuleId(null); return; }
    setExpandedRuleId(item.id);
    if (ruleHistories[item.id]) return;
    try {
      const history = await getInvestmentRuleHistory(item.id);
      setRuleHistories((current) => ({ ...current, [item.id]: history }));
    } catch (nextError) { setError(String(nextError)); }
  };

  const convertLessonToRule = (item: SystemReviewRecord) => {
    setEditingRuleId(null);
    setRule({
      category: "复盘",
      statement: "",
      trigger: "下次遇到相似决策时",
      rationale: item.lessons,
      active: true,
      sourceReviewId: item.id,
    });
    window.requestAnimationFrame(() => document.querySelector(".rule-editor")?.scrollIntoView({ behavior: "smooth", block: "center" }));
  };

  const startAiReview = () => {
    window.sessionStorage.setItem(
      "compass.advisorQuestion",
      "请基于我的个人投资规则、最近周期复盘、财务目标、当前组合和历史决策，完成一次系统复盘：先核对规则违反与风险边界，再识别重复错误，比较至少两种改进路径，并给出下一周期可验证的行动与证伪条件。",
    );
    navigate("advisor");
  };

  const toggleReminders = async () => {
    setReminderSaving(true); setError("");
    try {
      if (reminder?.enabled) {
        await disableReviewReminders();
        flash("桌面复盘提醒已关闭，本机到期队列仍会保留");
      } else {
        await enableReviewReminders();
        await checkAndSendReviewReminder();
        flash("桌面复盘提醒已开启");
      }
      await refresh();
    } catch (nextError) { setError(String(nextError)); } finally { setReminderSaving(false); }
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="方法论 · 校准层" title="让经验沉淀为规则" description="周期复盘不是解释盈亏，而是检查纪律、修订规则并冻结当时的证据。" action={<button className="primary" onClick={startAiReview}><Sparkles size={16} />AI 辅助系统复盘</button>} />
      <section className="review-metrics">
        <article><span>到期待复盘</span><strong className={dueDecisions.length ? "warning-text" : ""}>{dueDecisions.length}</strong><small>按原始证伪条件回看</small></article>
        <article><span>周期复盘</span><strong>{reviews.length}</strong><small className={periodicReviewDue ? "warning-text" : ""}>{periodicReviewDue ? "现在需要安排一次" : `下次 ${latestReview.nextReviewDate}`}</small></article>
        <article><span>有效规则</span><strong>{activeRules.length}</strong><small>{rules.length - activeRules.length} 条历史停用规则</small></article>
        <article><span>平均纪律评分</span><strong>{averageAdherence === null ? "—" : averageAdherence.toFixed(1)}</strong><small>只评价是否按流程行动</small></article>
      </section>

      <section className="panel reminder-panel">
        <div className="reminder-copy">
          <div className={`reminder-icon ${reminder?.enabled ? "enabled" : ""}`}>
            {reminder?.enabled ? <Bell size={18} /> : <BellOff size={18} />}
          </div>
          <div>
            <strong>桌面复盘提醒</strong>
            <p>应用打开时检查到期事项；相同到期状态每天最多提醒一次，锁屏通知不显示资产名称。</p>
            <small>设置只保存在这台设备，不参与云端同步。关闭应用后不会在后台运行。</small>
          </div>
        </div>
        <button className={reminder?.enabled ? "secondary" : "primary"} disabled={reminderSaving || !isDesktopApp()} onClick={toggleReminders}>
          {reminderSaving ? "处理中…" : reminder?.enabled ? "关闭提醒" : isDesktopApp() ? "开启提醒" : "仅桌面应用可用"}
        </button>
      </section>

      <section className="panel rule-effectiveness-panel">
        <div className="panel-title"><div><span>规则有效性追踪</span><h2>观察纪律与过程质量的关系</h2></div><small className="causality-note">只显示关联，不宣称因果</small></div>
        <div className="effectiveness-summary">
          <article><span>规则检查覆盖</span><strong>{effectiveness?.totalDecisions ? `${effectiveness.evaluatedDecisions}/${effectiveness.totalDecisions}` : "—"}</strong><small>启用此能力后的决策才计入</small></article>
          <article><span>适用规则遵守率</span><strong>{effectiveness?.adherencePct == null ? "—" : `${effectiveness.adherencePct.toFixed(0)}%`}</strong><small>不适用规则不进入分母</small></article>
          <article><span>已复盘规则样本</span><strong>{effectiveness?.reviewedChecks ?? 0}</strong><small>按决策过程评分比较</small></article>
        </div>
        {!effectiveness?.rules.length && <div className="empty">建立个人投资规则后，每次冻结决策都会先要求逐条确认。</div>}
        <div className="effectiveness-list">{effectiveness?.rules.map((item) => <article key={item.ruleId} className={item.active ? "" : "inactive"}>
          <div className="effectiveness-rule"><span>{item.category} · 当前 v{item.currentRevision}{item.active ? "" : " · 已停用"}</span><strong>{item.statement}</strong><small>{item.observedRevisions.length ? `已有决策覆盖版本 ${item.observedRevisions.join("、")}` : "尚无决策样本"}</small></div>
          <div className="effectiveness-counts"><span>适用 <b>{item.applicableCount}</b></span><span>遵守 <b>{item.followedCount}</b></span><span>偏离 <b>{item.deviatedCount}</b></span><span>已复盘 <b>{item.reviewedCount}</b></span></div>
          <div className="process-comparison"><span>遵守后的过程评分 <b>{item.followedProcessAverage == null ? "—" : item.followedProcessAverage.toFixed(1)}</b></span><span>偏离后的过程评分 <b>{item.deviatedProcessAverage == null ? "—" : item.deviatedProcessAverage.toFixed(1)}</b></span><strong className={item.signal.startsWith("反常") ? "warning-text" : ""}>{item.signal}</strong></div>
        </article>)}</div>
        <p className="effectiveness-disclaimer">过程评分也可能受规则本身影响，样本存在选择偏差。这里用于发现值得复核的规则，不用于证明某条规则提高收益。</p>
      </section>

      {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}

      {dueDecisions.length > 0 && <section className="panel due-review-panel">
        <div className="panel-title"><div><span>复盘队列</span><h2>先处理已经到期的原始判断</h2></div><button className="secondary" onClick={() => navigate("decision")}>前往决策日志</button></div>
        <div className="due-review-list">{dueDecisions.map((item) => <div key={item.id}><strong>{item.assetName}</strong><span>置信度 {item.confidencePct}%</span><span>计划复盘日 {item.reviewDate}</span><small>{item.invalidation}</small></div>)}</div>
      </section>}

      <section className="panel form-panel system-review-form">
        <div className="panel-title"><div><span>周期系统复盘</span><h2>冻结这一周期的过程与约束</h2></div><History size={21} className="muted-icon" /></div>
        <div className="form-grid">
          <label><span>复盘周期</span><input value={review.periodLabel} onChange={(e) => setReview({ ...review, periodLabel: e.target.value })} placeholder="例如 2026 Q3" /></label>
          <label><span>纪律执行评分</span><select value={review.adherenceScore} onChange={(e) => setReview({ ...review, adherenceScore: Number(e.target.value) })}>{[1, 2, 3, 4, 5].map((value) => <option key={value} value={value}>{value} / 5</option>)}</select></label>
          <label><span>下次复盘日</span><input type="date" value={review.nextReviewDate} onChange={(e) => setReview({ ...review, nextReviewDate: e.target.value })} /></label>
          <label className="span-3"><span>这一周期实际执行了什么？</span><textarea value={review.processSummary} onChange={(e) => setReview({ ...review, processSummary: e.target.value })} placeholder="只写事实：投入、再平衡、研究和计划外交易。" /></label>
          <label className="span-3"><span>违反了哪些预设规则？</span><textarea value={review.ruleViolations} onChange={(e) => setReview({ ...review, ruleViolations: e.target.value })} placeholder="没有则写“无”；不要用盈利为违规行为辩护。" /></label>
          <label className="span-3"><span>哪些认知需要修正？</span><textarea value={review.lessons} onChange={(e) => setReview({ ...review, lessons: e.target.value })} placeholder="区分可重复的经验与一次性噪声。" /></label>
          <label className="span-3"><span>下一周期只做哪些行动？</span><textarea value={review.nextActions} onChange={(e) => setReview({ ...review, nextActions: e.target.value })} placeholder="使用可检查的动作、期限和触发条件。" /></label>
        </div>
        <div className="form-actions"><p>保存时会同时冻结组合、目标、风险和决策完成度摘要。</p><button className="primary" disabled={saving || !review.processSummary || !review.lessons || !review.nextActions} onClick={persistSystemReview}><Save size={16} />冻结周期复盘</button></div>
      </section>

      <section className="panel rule-workbench">
        <div className="panel-title"><div><span>个人投资规则</span><h2>把经验写成触发时能执行的动作</h2></div><span className="history-count">{activeRules.length} 条有效</span></div>
        <div className="rule-layout">
          <div className="rule-list">
            {rules.length === 0 && <div className="empty">还没有个人规则。好的规则应说明“何时触发、具体做什么、为什么”。</div>}
            {rules.map((item) => <article key={item.id} className={item.active ? "" : "inactive"}>
              <div><span>{item.category} · v{item.revision}</span><strong>{item.statement}</strong><p><b>触发</b>{item.trigger}</p><small>{item.rationale}</small></div>
              <div className="rule-actions"><button className="text-button" onClick={() => toggleRuleHistory(item)}>历史</button><button className="text-button" onClick={() => { setEditingRuleId(item.id); setRule(ruleInput(item)); }}>修订</button><button className="text-button" disabled={saving} onClick={() => toggleRule(item)}>{item.active ? "停用" : "启用"}</button></div>
              {expandedRuleId === item.id && <div className="rule-history">{(ruleHistories[item.id] ?? []).map((revision) => <div key={revision.revision}><span>v{revision.revision} · {new Date(revision.changedAt).toLocaleDateString("zh-CN")}</span><strong>{revision.statement}</strong><small>{revision.active ? "当时启用" : "当时停用"}</small></div>)}</div>}
            </article>)}
          </div>
          <div className="rule-editor">
            <strong>{editingRuleId ? "修订规则" : rule.sourceReviewId ? "从复盘沉淀规则" : "建立一条规则"}</strong>
            <label><span>类别</span><select value={rule.category} onChange={(e) => setRule({ ...rule, category: e.target.value as InvestmentRuleInput["category"] })}>{["资产配置", "风险", "研究", "仓位", "行为", "复盘"].map((value) => <option key={value}>{value}</option>)}</select></label>
            <label><span>规则内容</span><textarea value={rule.statement} onChange={(e) => setRule({ ...rule, statement: e.target.value })} placeholder="例如：单一主动仓位不得超过 8%。" /></label>
            <label><span>触发条件</span><textarea value={rule.trigger} onChange={(e) => setRule({ ...rule, trigger: e.target.value })} placeholder="什么时候必须检查这条规则？" /></label>
            <label><span>依据</span><textarea value={rule.rationale} onChange={(e) => setRule({ ...rule, rationale: e.target.value })} placeholder="它避免哪一种重复错误？" /></label>
            <div className="rule-editor-actions">{(editingRuleId || rule.sourceReviewId) && <button className="text-button" onClick={() => { setEditingRuleId(null); setRule(emptyRule); }}>取消</button>}<button className="secondary" disabled={saving || !rule.statement || !rule.trigger || !rule.rationale} onClick={persistRule}>{editingRuleId ? "保存新版本" : "建立规则"}</button></div>
          </div>
        </div>
      </section>

      {reviews.length > 0 && <section className="panel system-review-history">
        <div className="panel-title"><div><span>冻结记录</span><h2>用当时的事实检验方法是否进步</h2></div><span className="history-count">{reviews.length} 期</span></div>
        <div className="system-review-list">{reviews.map((item) => <article key={item.id}>
          <div className="system-review-head"><div><span>{item.periodLabel}</span><strong>纪律 {item.adherenceScore}/5</strong></div><small>{new Date(item.createdAt).toLocaleDateString("zh-CN")}</small></div>
          <p><b>过程事实</b>{item.processSummary}</p><p><b>规则违反</b>{item.ruleViolations || "无"}</p><p><b>经验修正</b>{item.lessons}</p><p><b>下一步</b>{item.nextActions}</p>
          <div className="frozen-snapshot"><span>组合 {money.format(item.snapshot.portfolioValue)}</span><span>集中度 {item.snapshot.concentrationPct.toFixed(1)}%</span><span>高风险 {item.snapshot.highRiskFindings}</span><span>目标 {item.snapshot.goalsOnTrack}/{item.snapshot.goalTotal}</span></div>
          <button className="text-button" onClick={() => convertLessonToRule(item)}>把经验沉淀为规则 <ChevronRight size={14} /></button>
        </article>)}</div>
      </section>}
    </div>
  );
}

function Advisor({ model, navigate, requestedAnalysisId, clearRequestedAnalysis, onCreateDecisionDraft }: { model: ModelConfig; navigate: (v: View) => void; requestedAnalysisId: string | null; clearRequestedAnalysis: () => void; onCreateDecisionDraft: (draft: DecisionEntry) => void }) {
  const [question, setQuestion] = useState("请基于我的财务目标和当前组合，指出最需要优先处理的风险，并给出不依赖市场预测的改进方案。");
  const [deep, setDeep] = useState(true);
  const [memory, setMemory] = useState(true);
  const [reflection, setReflection] = useState(true);
  const [alternatives, setAlternatives] = useState(true);
  const [excludedMemoryIds, setExcludedMemoryIds] = useState<string[]>([]);
  const [contextSelection, setContextSelection] = useState<ContextSelection>({ includeProfile: true, includeGoals: true, includeHoldings: true, includePlanning: true, includeRiskFindings: true, includeRules: true, includeSystemReviews: true, includeEvidence: true });
  const [preview, setPreview] = useState<AnalysisPreview | null>(null);
  const [previewStale, setPreviewStale] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [error, setError] = useState("");
  const [history, setHistory] = useState<AnalysisHistoryItem[]>([]);
  const [storedAnalysis, setStoredAnalysis] = useState<StoredAnalysis | null>(null);
  const [historyBusy, setHistoryBusy] = useState("");
  const [historyError, setHistoryError] = useState("");

  useEffect(() => {
    const queued = window.sessionStorage.getItem("compass.advisorQuestion");
    if (queued) {
      setQuestion(queued);
      window.sessionStorage.removeItem("compass.advisorQuestion");
    }
  }, []);

  useEffect(() => {
    getAnalysisHistory()
      .then(setHistory)
      .catch((nextError) => setHistoryError(String(nextError)));
  }, []);

  useEffect(() => {
    if (!requestedAnalysisId) return;
    setHistoryBusy(requestedAnalysisId);
    setHistoryError("");
    setResult(null);
    setPreview(null);
    getAnalysis(requestedAnalysisId)
      .then((item) => {
        setStoredAnalysis(item);
        window.setTimeout(() => document.getElementById("stored-analysis")?.scrollIntoView({ behavior: "smooth", block: "start" }), 50);
      })
      .catch((nextError) => setHistoryError(String(nextError)))
      .finally(() => {
        setHistoryBusy("");
        clearRequestedAnalysis();
      });
  }, [requestedAnalysisId]);

  const request = (previewRevision?: string): AnalysisRequest => ({
    question,
    workflow: deep ? "deep" : "quick",
    useMemory: memory,
    reflect: reflection,
    exploreAlternatives: alternatives,
    excludedMemoryIds,
    contextSelection,
    previewRevision,
  });

  const invalidatePreview = (action: () => void) => {
    action();
    setExcludedMemoryIds([]);
    setPreview(null);
    setPreviewStale(false);
    setResult(null);
    setStoredAnalysis(null);
  };

  const prepare = async () => {
    setPreviewing(true); setError(""); setResult(null);
    try { setPreview(await previewAnalysis(request())); setPreviewStale(false); }
    catch (e) { setError(String(e)); } finally { setPreviewing(false); }
  };

  const analyze = async () => {
    setRunning(true); setError(""); setResult(null);
    try {
      if (!preview) throw new Error("请先预览将发送的数据");
      if (previewStale) throw new Error("记忆选择已变化，请重新预览后再确认");
      const nextResult = await runAnalysis(request(preview.contextRevision));
      setResult(nextResult);
      setStoredAnalysis(null);
      setPreview(null);
      getAnalysisHistory().then(setHistory).catch(() => undefined);
    } catch (e) { setError(String(e)); } finally { setRunning(false); }
  };

  const toggleContext = (key: keyof ContextSelection) => invalidatePreview(() => setContextSelection((current) => ({ ...current, [key]: !current[key] })));
  const toggleMemoryCandidate = (id: string) => {
    setExcludedMemoryIds((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]);
    setPreviewStale(true);
    setResult(null);
  };

  const openStoredAnalysis = async (id: string) => {
    setHistoryBusy(id); setHistoryError(""); setResult(null); setPreview(null);
    try {
      setStoredAnalysis(await getAnalysis(id));
      window.setTimeout(() => document.getElementById("stored-analysis")?.scrollIntoView({ behavior: "smooth", block: "start" }), 50);
    } catch (nextError) { setHistoryError(String(nextError)); }
    finally { setHistoryBusy(""); }
  };

  const reuseStoredQuestion = (item: StoredAnalysis) => {
    setQuestion(item.question);
    setStoredAnalysis(null);
    setResult(null);
    setPreview(null);
    setPreviewStale(false);
    window.scrollTo({ top: 0, behavior: "smooth" });
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="AI 原生分析" title="研究室，而不是荐股机" description="规则引擎先处理确定性风险，大模型负责理解、比较、反驳与解释。" action={<div className={`model-pill ${model.hasApiKey ? "ready" : ""}`}><Bot size={15} />{model.hasApiKey ? model.model : "尚未配置模型"}</div>} />
      {!model.hasApiKey && <div className="setup-banner"><KeyRound size={20} /><div><strong>配置自己的模型密钥</strong><p>密钥保存到系统钥匙串，不写入投资数据库。</p></div><button className="secondary" onClick={() => navigate("settings")}>立即配置</button></div>}
      <section className="panel advisor-panel">
        <label className="question-box"><span>这次希望解决什么问题？</span><textarea value={question} onChange={(e) => invalidatePreview(() => setQuestion(e.target.value))} /></label>
        <div className="workflow-options">
          <Toggle icon={<BrainCircuit size={17} />} title="深度编排" detail="构建计划并分阶段分析" checked={deep} onChange={(value) => invalidatePreview(() => setDeep(value))} />
          <Toggle icon={<Database size={17} />} title="本地记忆" detail="检索相关历史决策" checked={memory} onChange={(value) => invalidatePreview(() => setMemory(value))} />
          <Toggle icon={<ShieldCheck size={17} />} title="纠错反思" detail="独立检查遗漏和过度自信" checked={reflection} onChange={(value) => invalidatePreview(() => setReflection(value))} />
          <Toggle icon={<Sparkles size={17} />} title="多方案探索" detail="比较至少两条可行路径" checked={alternatives} onChange={(value) => invalidatePreview(() => setAlternatives(value))} />
        </div>
        <div className="context-control">
          <div><strong>选择允许发送的本地上下文</strong><span>取消选择后，该组不会进入模型提示词</span></div>
          <div className="context-options">
            {([
              ["includeProfile", "财务档案"], ["includeGoals", "目标计划"], ["includeHoldings", "持仓明细"],
              ["includePlanning", "规划结果"], ["includeRiskFindings", "风险检查"], ["includeRules", "个人规则"],
              ["includeSystemReviews", "周期复盘"], ["includeEvidence", "研究证据"],
            ] as [keyof ContextSelection, string][]).map(([key, label]) => <button key={key} className={contextSelection[key] ? "selected" : ""} onClick={() => toggleContext(key)}><i>{contextSelection[key] && <Check size={11} />}</i>{label}</button>)}
          </div>
        </div>
        <button className="primary analyze-button" onClick={prepare} disabled={previewing || running || !question.trim()}>
          {previewing ? <><LoaderCircle size={17} className="spin" />正在生成本地预览…</> : <><Eye size={17} />预览将发送的数据</>}
        </button>
      </section>

      {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}
      {historyError && <div className="error-box"><AlertTriangle size={18} />{historyError}</div>}
      {storedAnalysis && <StoredAnalysisView item={storedAnalysis} onClose={() => setStoredAnalysis(null)} onReuse={() => reuseStoredQuestion(storedAnalysis)} onCreateDecisionDraft={onCreateDecisionDraft} />}
      {preview && <section className="panel preview-panel">
        <div className="panel-title"><div><span>发送前确认</span><h2>模型将看到这些内容</h2></div><div className="preview-size">{(preview.payloadBytes / 1024).toFixed(1)} KB</div></div>
        <div className="preview-provider"><Bot size={16} /><span><strong>{preview.model}</strong>{preview.provider} · {preview.workflow === "deep" ? "深度工作流" : "快速工作流"}</span></div>
        {previewStale && <div className="preview-stale"><AlertTriangle size={15} /><div><strong>记忆授权已变化</strong><span>下方显示的是新选择，但旧指纹已经失效。重新预览后才能开始分析。</span></div></div>}
        <div className="context-group-list">{preview.groups.map((group) => <div className={group.included ? "included" : "omitted"} key={group.key}><i>{group.included ? <Check size={12} /> : "—"}</i><div><strong>{group.label}</strong><small>{group.description}</small></div><span>{group.included ? `${group.recordCount} 项 · ${group.sensitivity}` : "留在本机"}</span></div>)}</div>
        <div className="local-only-note"><LockKeyhole size={16} /><div><strong>始终留在本机</strong><p>{preview.localOnly.join("；")}</p></div></div>
        <details className="payload-details"><summary>查看实际本地数据载荷</summary><pre>{JSON.stringify(preview.payload, null, 2)}</pre></details>
        {preview.evidenceCandidates.length > 0 && <details className="payload-details"><summary>查看本次可引用证据（{preview.evidenceCandidates.length} 条）</summary><pre>{JSON.stringify(preview.evidenceCandidates, null, 2)}</pre></details>}
        {preview.memoryCandidates.length > 0 && <details className="payload-details memory-disclosure"><summary>逐条选择候选记忆（授权 {preview.memoryCandidates.filter((item) => !excludedMemoryIds.includes(item.id)).length}/{preview.memoryCandidates.length} 条）</summary><MemoryItems items={preview.memoryCandidates} excludedIds={excludedMemoryIds} onToggle={toggleMemoryCandidate} /></details>}
        <details className="payload-details"><summary>查看固定投资方法论提示</summary><pre>{preview.systemPolicy}</pre></details>
        <p className="memory-policy">{preview.memoryPolicy}</p>
        <div className="preview-actions"><button className="text-button" onClick={() => setPreview(null)}>返回修改</button><button className="primary" onClick={previewStale ? prepare : analyze} disabled={running || previewing || (!previewStale && !model.hasApiKey)}>{previewing ? <><LoaderCircle size={15} className="spin" />正在更新预览…</> : previewStale ? <><Eye size={15} />按新选择重新预览</> : running ? <><LoaderCircle size={15} className="spin" />正在分析…</> : <><Send size={15} />确认并开始分析</>}</button></div>
      </section>}
      {result && <section className="panel result-panel">
        <div className="result-meta">{result.stages.map((stage) => <span key={stage}><Check size={13} />{stage}</span>)}</div>
        <div className="analysis-audit"><div><strong>{result.transparency.model}</strong><span>{result.transparency.provider}</span></div><div><strong>{result.transparency.modelCalls} 次</strong><span>模型调用</span></div><div><strong>{(result.transparency.totalLatencyMs / 1000).toFixed(1)} 秒</strong><span>模型总耗时</span></div><div><strong>{result.transparency.inputTokens == null ? "未返回" : result.transparency.inputTokens.toLocaleString()}</strong><span>输入 tokens</span></div><div><strong>{result.transparency.contextGroups.length} 组</strong><span>上下文</span></div><div><strong>{result.transparency.memoryItemsUsed} 条</strong><span>采用记忆</span></div><div><strong>{result.transparency.reviewedMemoryItemsUsed} / {result.transparency.conflictingMemoryItemsUsed}</strong><span>已复盘 / 反证</span></div><div><strong>{result.transparency.evidenceItemsUsed} 条</strong><span>带来源证据</span></div><div><strong>{result.transparency.citationsRequired ? "外部事实须引用" : "无可引用证据"}</strong><span>引用约束</span></div><div><strong>{result.transparency.structuredOutputValidated ? (result.transparency.outputRepairs > 0 ? `修复 ${result.transparency.outputRepairs} 次` : "直接通过") : "未校验"}</strong><span>输出契约</span></div><div><strong>{result.transparency.apiKeySent ? "异常" : "未进入提示词"}</strong><span>API Key</span></div></div>
        {(result.workflowTrace.researchPlan || result.workflowTrace.alternatives.length > 0 || result.workflowTrace.critique) && <div className="workflow-trace">
          <div className="workflow-trace-title"><div><span>可审计工作流 · {result.workflowTrace.version}</span><strong>查看模型如何比较、反驳再裁决</strong></div><small>{result.workflowTrace.outputValidation ? `最终输出 ${result.workflowTrace.outputValidation.status === "repaired" ? "经 1 次自动修复后" : "首次"}通过机器校验。` : "以下是显式要求模型输出的研究产物，不是隐藏思维过程。"}</small></div>
          {result.workflowTrace.researchPlan && <details><summary><span>01</span><div><strong>研究计划</strong><small>假设、未知与检索线索</small></div><ChevronRight size={15} /></summary><div className="trace-content">{result.workflowTrace.researchPlan}</div></details>}
          {result.workflowTrace.memoryItems.length > 0 && <details><summary><span>M</span><div><strong>实际采用的长期记忆</strong><small>{result.workflowTrace.memoryItems.length} 条 · 显示命中原因与冲突信号</small></div><ChevronRight size={15} /></summary><MemoryItems items={result.workflowTrace.memoryItems} compact /></details>}
          {result.workflowTrace.alternatives.map((alternative, index) => <details key={alternative.id}><summary><span>{String(index + 2).padStart(2, "0")}</span><div><strong>{alternative.label}</strong><small>{alternative.lens}</small></div><ChevronRight size={15} /></summary><div className="trace-content">{alternative.content}</div></details>)}
          {result.workflowTrace.critique && <details><summary><span>{String(result.workflowTrace.alternatives.length + 2).padStart(2, "0")}</span><div><strong>独立风险审查</strong><small>寻找证据漏洞、极端风险与过度自信</small></div><ChevronRight size={15} /></summary><div className="trace-content">{result.workflowTrace.critique}</div></details>}
          <details className="call-trace"><summary><span>Σ</span><div><strong>模型调用记录</strong><small>{result.workflowTrace.calls.length} 个独立阶段</small></div><ChevronRight size={15} /></summary><div className="call-list">{result.workflowTrace.calls.map((call) => <div key={call.stage}><strong>{call.label}</strong><span>{(call.latencyMs / 1000).toFixed(2)} 秒</span><span>{call.inputTokens == null ? "token 未返回" : `${call.inputTokens.toLocaleString()} 入 / ${(call.outputTokens ?? 0).toLocaleString()} 出`}</span></div>)}</div></details>
        </div>}
        <div className="final-answer-label"><Sparkles size={15} /><div><span>最终综合裁决</span><strong>吸收方案与反方审查后的行动建议</strong></div></div>
        {result.workflowTrace.outputValidation && <details className="payload-details"><summary>输出检查：{result.workflowTrace.outputValidation.status === "repaired" ? "修复后通过" : "首次通过"}</summary><p className="memory-policy">已检查字段完整性和引用 ID 是否属于本次授权证据；不代表事实准确性或推理有效性已经得到验证。</p>{result.workflowTrace.outputValidation.errors.length > 0 && <pre>{result.workflowTrace.outputValidation.errors.join("\n")}</pre>}</details>}
        {result.workflowTrace.structuredReport ? <StructuredReportView report={result.workflowTrace.structuredReport} evidence={result.workflowTrace.evidenceCatalog ?? []} analysisId={result.id} onCreateDecisionDraft={onCreateDecisionDraft} /> : <div className="answer">{result.answer}</div>}<p className="disclaimer">{result.disclaimer}</p>
      </section>}
      <section className="panel analysis-history-panel">
        <div className="panel-title"><div><span>本地分析档案</span><h2>回看当时的问题，而不是依赖记忆改写</h2></div><span className="history-count">最近 {history.length} 条</span></div>
        <p className="analysis-history-boundary">历史 AI 分析是未经结果验证的研究产物。它可以被重开、追溯或转成待确认草稿，但不会自动成为事实、规则或交易指令。</p>
        {history.length === 0 && !historyError && <div className="empty">还没有保存过 AI 分析。成功完成一次分析后，完整工作流会留在本机。</div>}
        <div className="analysis-history-list">{history.map((item) => <button key={item.id} className={storedAnalysis?.id === item.id ? "active" : ""} onClick={() => void openStoredAnalysis(item.id)} disabled={Boolean(historyBusy)}><div><History size={15} /><span>{item.workflowVersion || "旧版分析"}</span></div><strong>{item.question}</strong><p>{item.verdict || "旧记录没有结构化裁决，可打开查看原回答。"}</p><small>{new Date(item.createdAt).toLocaleString("zh-CN")} · {item.transparency?.model || "无模型审计"}</small><em>{historyBusy === item.id ? <LoaderCircle size={14} className="spin" /> : <ChevronRight size={14} />}</em></button>)}</div>
      </section>
    </div>
  );
}

function StoredAnalysisView({ item, onClose, onReuse, onCreateDecisionDraft }: { item: StoredAnalysis; onClose: () => void; onReuse: () => void; onCreateDecisionDraft: (draft: DecisionEntry) => void }) {
  const trace = item.workflowTrace;
  const audit = item.transparency;
  return <section className="panel stored-analysis" id="stored-analysis">
    <div className="panel-title stored-analysis-title"><div><span>冻结于 {new Date(item.createdAt).toLocaleString("zh-CN")}</span><h2>{item.question}</h2></div><div><button className="text-button" onClick={onClose}>关闭</button><button className="secondary" onClick={onReuse}>用当前数据重新分析</button></div></div>
    <div className="historical-analysis-warning"><History size={17} /><div><strong>历史 AI 分析 · 未经结果验证</strong><p>这是当时保存的原始研究产物，不会按当前持仓或新证据自动更新。重新分析会重新生成发送前预览。</p></div></div>
    {audit && <div className="analysis-audit stored-analysis-audit"><div><strong>{audit.model}</strong><span>{audit.provider}</span></div><div><strong>{audit.modelCalls || "—"} 次</strong><span>模型调用</span></div><div><strong>{audit.totalLatencyMs ? `${(audit.totalLatencyMs / 1000).toFixed(1)} 秒` : "—"}</strong><span>模型总耗时</span></div><div><strong>{audit.memoryItemsUsed} 条</strong><span>采用记忆</span></div><div><strong>{audit.evidenceItemsUsed} 条</strong><span>带来源证据</span></div><div><strong>{audit.structuredOutputValidated ? (audit.outputRepairs > 0 ? `修复 ${audit.outputRepairs} 次` : "直接通过") : "旧版/未校验"}</strong><span>输出契约</span></div></div>}
    {trace && <StoredWorkflowTrace trace={trace} />}
    <div className="final-answer-label"><Sparkles size={15} /><div><span>当时的综合裁决</span><strong>请结合当前事实重新判断，不把旧回答当作实时建议</strong></div></div>
    {trace?.outputValidation && <details className="payload-details"><summary>输出检查：{trace.outputValidation.status === "repaired" ? "修复后通过" : "首次通过"}</summary><p className="memory-policy">这里只证明输出符合当时的字段与引用契约，不证明事实、推理或未来结果正确。</p>{trace.outputValidation.errors.length > 0 && <pre>{trace.outputValidation.errors.join("\n")}</pre>}</details>}
    {trace?.structuredReport ? <StructuredReportView report={trace.structuredReport} evidence={trace.evidenceCatalog ?? []} analysisId={item.id} onCreateDecisionDraft={onCreateDecisionDraft} /> : <div className="answer">{item.answer}</div>}
    <p className="disclaimer">历史记录仅用于复盘当时的研究过程，不构成投资建议或收益保证。</p>
  </section>;
}

function StoredWorkflowTrace({ trace }: { trace: AnalysisWorkflowTrace }) {
  if (!trace.researchPlan && trace.alternatives.length === 0 && !trace.critique && trace.calls.length === 0) return null;
  return <div className="workflow-trace">
    <div className="workflow-trace-title"><div><span>可审计工作流 · {trace.version || "旧版"}</span><strong>当时实际保存的研究阶段</strong></div><small>以下是模型被明确要求输出的工作产物，不是隐藏思维过程。</small></div>
    {trace.researchPlan && <details><summary><span>01</span><div><strong>研究计划</strong><small>假设、未知与检索线索</small></div><ChevronRight size={15} /></summary><div className="trace-content">{trace.researchPlan}</div></details>}
    {trace.memoryItems.length > 0 && <details><summary><span>M</span><div><strong>实际采用的长期记忆</strong><small>{trace.memoryItems.length} 条 · 保留当时命中原因</small></div><ChevronRight size={15} /></summary><MemoryItems items={trace.memoryItems} compact /></details>}
    {trace.alternatives.map((alternative, index) => <details key={alternative.id}><summary><span>{String(index + 2).padStart(2, "0")}</span><div><strong>{alternative.label}</strong><small>{alternative.lens}</small></div><ChevronRight size={15} /></summary><div className="trace-content">{alternative.content}</div></details>)}
    {trace.critique && <details><summary><span>{String(trace.alternatives.length + 2).padStart(2, "0")}</span><div><strong>独立风险审查</strong><small>证据漏洞、极端风险与过度自信</small></div><ChevronRight size={15} /></summary><div className="trace-content">{trace.critique}</div></details>}
    {trace.calls.length > 0 && <details className="call-trace"><summary><span>Σ</span><div><strong>模型调用记录</strong><small>{trace.calls.length} 个独立阶段</small></div><ChevronRight size={15} /></summary><div className="call-list">{trace.calls.map((call) => <div key={call.stage}><strong>{call.label}</strong><span>{(call.latencyMs / 1000).toFixed(2)} 秒</span><span>{call.inputTokens == null ? "token 未返回" : `${call.inputTokens.toLocaleString()} 入 / ${(call.outputTokens ?? 0).toLocaleString()} 出`}</span></div>)}</div></details>}
  </div>;
}

function StructuredReportView({ report, evidence, analysisId, onCreateDecisionDraft }: { report: StructuredAnalysis; evidence: AnalysisEvidenceReference[]; analysisId: string; onCreateDecisionDraft: (draft: DecisionEntry) => void }) {
  const evidenceById = new Map(evidence.map((item) => [item.id, item]));
  const createDecisionDraft = (action: AnalysisAction, index: number) => {
    const counterPoints = [
      ...report.options.flatMap((option) => option.risks),
      ...report.unknowns,
    ].filter((item, itemIndex, all) => item.trim() && all.indexOf(item) === itemIndex);
    const invalidation = [action.reviewTrigger, ...report.reviewTriggers]
      .filter((item, itemIndex, all) => item.trim() && all.indexOf(item) === itemIndex)
      .join("\n");
    onCreateDecisionDraft({
      ...emptyDecision,
      sourceAnalysisId: analysisId,
      sourceActionIndex: index,
      thesis: `${report.verdict}\n\n拟采取行动：${action.action}\n理由：${action.rationale}`,
      counterThesis: counterPoints.join("\n"),
      invalidation,
    });
  };
  return <div className="structured-report">
    <section className="report-verdict"><span>当前最重要判断</span><p>{report.verdict}</p></section>
    <div className="report-claim-grid">
      <section><div className="report-section-title"><Check size={14} /><strong>已知事实</strong><small>用户数据或已授权证据</small></div><ClaimList items={report.facts} evidenceById={evidenceById} /></section>
      <section><div className="report-section-title"><BrainCircuit size={14} /><strong>合理推断</strong><small>不与事实混写</small></div>{report.inferences.length > 0 ? <ClaimList items={report.inferences} evidenceById={evidenceById} /> : <p className="report-empty">本次没有需要单列的推断。</p>}</section>
    </div>
    <section className="report-section unknown-section"><div className="report-section-title"><AlertTriangle size={14} /><strong>仍待核实</strong><small>模型不得补写为事实</small></div><ul>{report.unknowns.map((item, index) => <li key={`${item}-${index}`}>{item}</li>)}</ul></section>
    <section className="report-section"><div className="report-section-title"><Compass size={14} /><strong>方案与取舍</strong><small>{report.options.length} 条可行路径</small></div><div className="report-option-grid">{report.options.map((option, index) => <article key={`${option.name}-${index}`}><span>方案 {String(index + 1).padStart(2, "0")}</span><h3>{option.name}</h3><p><b>适用条件</b>{option.suitableWhen}</p><p><b>机会成本</b>{option.tradeoffs.join("；")}</p><p><b>主要风险</b>{option.risks.join("；")}</p></article>)}</div></section>
    <section className="report-section"><div className="report-section-title"><ArrowRight size={14} /><strong>下一步行动</strong><small>选择后仍需人工补全与确认</small></div><div className="report-action-list">{report.actions.map((action, index) => <article key={`${action.action}-${index}`}><i>{index + 1}</i><div><strong>{action.action}</strong><p>{action.rationale}</p><small>复盘：{action.reviewTrigger}</small><button className="decision-draft-button" onClick={() => createDecisionDraft(action, index)}><FilePenLine size={12} />转为决策草稿</button></div><em className={action.reversible ? "reversible" : "confirm-first"}>{action.reversible ? "可逆" : "需单独确认"}</em></article>)}</div></section>
    <section className="report-section trigger-section"><div className="report-section-title"><History size={14} /><strong>复盘与证伪条件</strong><small>未来用结果校准判断</small></div><ul>{report.reviewTriggers.map((item, index) => <li key={`${item}-${index}`}>{item}</li>)}</ul></section>
  </div>;
}

function ClaimList({ items, evidenceById }: { items: AnalysisClaim[]; evidenceById: Map<string, AnalysisEvidenceReference> }) {
  return <div className="report-claim-list">{items.map((claim, index) => <article key={`${claim.statement}-${index}`}>
    <p>{claim.statement}</p>
    <footer><span>{claim.basis === "research_evidence" ? "研究证据" : "用户数据"}</span>{claim.evidenceIds.map((id) => {
      const source = evidenceById.get(id);
      return source ? <a key={id} href={source.sourceUrl} target="_blank" rel="noreferrer" title={`${source.publisher} · ${source.asOfDate}`}>{source.title} · {source.sourceTier} · {source.asOfDate}</a> : <em key={id}>证据 {id}</em>;
    })}</footer>
  </article>)}</div>;
}

function MemoryItems({ items, compact = false, excludedIds = [], onToggle }: { items: MemoryCandidate[]; compact?: boolean; excludedIds?: string[]; onToggle?: (id: string) => void }) {
  return <div className={`memory-candidate-list ${compact ? "compact" : ""}`}>{items.map((item) => {
    const selected = onToggle ? !excludedIds.includes(item.id) : item.selected;
    return <article key={`${item.kind}-${item.id}`} className={`${item.contradiction ? "contradiction" : ""} ${selected ? "" : "excluded"}`}>
    <div className="memory-head"><div><span>{item.kind === "decision" ? "决策" : "AI 分析"}</span><strong>{item.title}</strong></div><div>{item.reviewed && <em>已复盘</em>}{item.contradiction && <em className="counter">反证</em>}{item.retrieval && <b>{item.retrieval.score.toFixed(1)} 分</b>}{onToggle && <button className={selected ? "memory-toggle selected" : "memory-toggle"} onClick={() => onToggle(item.id)}>{selected ? "本次发送" : "留在本机"}</button>}</div></div>
    <p>{item.summary}</p>
    <div className="memory-reasons">{item.retrieval?.reasons.map((reason) => <span key={reason}>{reason}</span>)}</div>
    <footer><small>{new Date(item.occurredAt).toLocaleDateString("zh-CN")} · {item.status}</small>{selected && item.retrieval ? <small>{item.retrieval.passes.join(" + ") || "候选初筛"}</small> : <small className="local-memory">不会进入模型</small>}</footer>
    {!compact && <details><summary>查看冻结的结构化内容</summary><pre>{JSON.stringify(item.content, null, 2)}</pre></details>}
  </article>;})}</div>;
}

function Toggle({ icon, title, detail, checked, onChange }: { icon: React.ReactNode; title: string; detail: string; checked: boolean; onChange: (v: boolean) => void }) {
  return <button className={`toggle-card ${checked ? "selected" : ""}`} onClick={() => onChange(!checked)}><span className="toggle-icon">{icon}</span><div><strong>{title}</strong><small>{detail}</small></div><i>{checked && <Check size={13} />}</i></button>;
}

function CloudSync({ flash }: { flash: (message: string) => void }) {
  const [status, setStatus] = useState<CloudStatus | null>(null);
  const [url, setUrl] = useState("");
  const [publishableKey, setPublishableKey] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [recoveryKey, setRecoveryKey] = useState("");
  const [revealedKey, setRevealedKey] = useState("");
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [result, setResult] = useState("");

  const refresh = async () => setStatus(await getCloudStatus());

  useEffect(() => {
    Promise.all([getCloudConfig(), getCloudStatus()])
      .then(([config, nextStatus]) => {
        if (config) { setUrl(config.url); setPublishableKey(config.publishableKey); }
        setStatus(nextStatus);
      })
      .catch((nextError) => setError(String(nextError)));
  }, []);

  const execute = async (name: string, action: () => Promise<string>) => {
    setBusy(name); setError(""); setResult("");
    try { const message = await action(); setResult(message); }
    catch (nextError) { setError(String(nextError)); }
    finally { setBusy(""); }
  };

  const persistConfig = () => execute("config", async () => {
    const next = await saveCloudConfig({ url, publishableKey });
    setStatus(next); flash("云端服务配置已保存");
    return "服务地址只用于账户认证和加密数据包同步。";
  });

  const authenticate = (mode: "signup" | "login") => execute(mode, async () => {
    const response = mode === "signup" ? await signUpCloud(email, password) : await signInCloud(email, password);
    setPassword(""); await refresh(); flash(response.message);
    return response.message;
  });

  const logout = () => execute("logout", async () => {
    setStatus(await signOutCloud()); setPassword("");
    return "已退出账户；本地投资数据和恢复密钥仍保留在此设备。";
  });

  const push = () => execute("push", async () => {
    const response = await pushCloudSync(); await refresh();
    flash(`已上传云端版本 ${response.revision}`);
    return `${response.message}（${response.recordCount} 条记录）`;
  });

  const pull = async () => {
    if (!window.confirm("拉取会以云端快照替换本机的投资域数据。模型配置、模型密钥和账户配置不会改变。确认继续？")) return;
    await execute("pull", async () => {
      const response = await pullCloudSync(true); await refresh();
      flash(`已恢复云端版本 ${response.revision}`);
      window.setTimeout(() => window.location.reload(), 900);
      return `${response.message}（${response.recordCount} 条记录）`;
    });
  };

  const revealRecoveryKey = async () => {
    if (!window.confirm("恢复密钥可以解密你的云端投资数据。仅在私密环境中显示，并请离线保管。继续？")) return;
    await execute("reveal", async () => {
      const response = await exportCloudRecoveryKey(); setRevealedKey(response.recoveryKey);
      return response.warning;
    });
  };

  const copyRecoveryKey = async () => {
    await navigator.clipboard.writeText(revealedKey); flash("恢复密钥已复制，请离线保管");
  };

  const importRecoveryKey = async () => {
    if (status?.hasRecoveryKey && !window.confirm("本机已有恢复密钥。替换后，原密钥对应的云端密文可能无法在本机解密。确认替换？")) return;
    await execute("import", async () => {
      await importCloudRecoveryKey(recoveryKey, Boolean(status?.hasRecoveryKey));
      setRecoveryKey(""); await refresh(); flash("恢复密钥已保存到系统钥匙串");
      return "现在可以拉取同一账户的云端加密快照。";
    });
  };

  if (!status) return <div className="center-screen"><LoaderCircle className="spin" /><span>正在读取账户与同步状态…</span></div>;

  return <div className="page narrow cloud-page">
    <PageHeader eyebrow="账户与同步" title="跨设备延续同一套投资系统" description="账户负责识别你；投资数据在离开设备前完成加密，云端无法读取明文。" action={<div className={`status-dot ${status.signedIn ? "connected" : ""}`}>{status.signedIn ? status.email : "未登录"}</div>} />

    <section className="panel form-panel">
      <div className="panel-title"><div><span>可替换提供商 · Supabase 参考实现</span><h2>云端服务</h2></div><Cloud size={22} className="muted-icon" /></div>
      <div className="form-grid single-column">
        <label><span>Project URL</span><input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="https://your-project.supabase.co" /></label>
        <label><span>Publishable / anon Key</span><div className="secure-input"><KeyRound size={16} /><input value={publishableKey} onChange={(event) => setPublishableKey(event.target.value)} placeholder="公开客户端 Key，不要填写 service_role Key" /></div></label>
      </div>
      <div className="privacy-note"><ShieldCheck size={18} /><div><strong>不要使用 service_role Key</strong><p>客户端只需要 Publishable/anon Key；数据库由 RLS 和带版本比较的写入函数限制。</p></div></div>
      <div className="form-actions"><p>更换服务地址会退出当前账户并清除同步基线，不删除本地数据。</p><button className="primary" onClick={persistConfig} disabled={busy !== "" || !url || !publishableKey}><Save size={16} />保存服务配置</button></div>
    </section>

    <section className="panel form-panel">
      <div className="panel-title"><div><span>设备绑定</span><h2>{status.signedIn ? "当前账户" : "注册或登录"}</h2></div><UserRound size={22} className="muted-icon" /></div>
      {status.signedIn ? <div className="account-card"><div><strong>{status.email}</strong><span>登录令牌仅保存在系统钥匙串</span></div><button className="danger-text" onClick={logout} disabled={busy !== ""}><LogOut size={14} />退出账户</button></div> : <>
        <div className="form-grid single-column">
          <label><span>邮箱</span><input type="email" value={email} onChange={(event) => setEmail(event.target.value)} placeholder="name@example.com" /></label>
          <label><span>密码</span><div className="secure-input"><LockKeyhole size={16} /><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder="至少 8 个字符；不会保存到本机" /></div></label>
        </div>
        {status.emailConfirmationPending && <div className="connection-success"><Send size={16} /><span><strong>等待邮箱确认</strong>确认后回到这里登录</span></div>}
        <div className="form-actions"><p>账户系统由你配置的服务提供商托管。</p><div className="key-actions"><button className="secondary" onClick={() => authenticate("signup")} disabled={busy !== "" || !status.configured || !email || password.length < 8}>创建账户</button><button className="primary" onClick={() => authenticate("login")} disabled={busy !== "" || !status.configured || !email || password.length < 8}>登录</button></div></div>
      </>}
    </section>

    <section className="panel form-panel">
      <div className="panel-title"><div><span>端到端加密 · 手动触发</span><h2>同步控制台</h2></div><div className={`status-dot ${!status.localChangedSinceSync && status.baseRevision > 0 ? "connected" : ""}`}>{status.baseRevision > 0 ? `云端版本 ${status.baseRevision}` : "尚未同步"}</div></div>
      <div className="sync-summary"><article><span>本机状态</span><strong>{status.localChangedSinceSync ? "有待同步修改" : "与同步基线一致"}</strong></article><article><span>上次成功同步</span><strong>{status.lastSyncedAt ? new Date(status.lastSyncedAt).toLocaleString("zh-CN") : "无"}</strong></article><article><span>恢复密钥</span><strong>{status.hasRecoveryKey ? "已存入钥匙串" : "尚未生成 / 导入"}</strong></article></div>
      <div className="sync-actions"><button className="primary" onClick={push} disabled={busy !== "" || !status.signedIn}><Upload size={16} />上传加密快照</button><button className="secondary" onClick={pull} disabled={busy !== "" || !status.signedIn || !status.hasRecoveryKey}><Download size={16} />拉取并替换本机数据</button></div>
      <div className="privacy-boundary">{status.privacyBoundary.map((item) => <div key={item}><Check size={13} />{item}</div>)}</div>
    </section>

    <section className="panel form-panel recovery-panel">
      <div className="panel-title"><div><span>跨设备恢复</span><h2>恢复密钥</h2></div><KeyRound size={22} className="muted-icon" /></div>
      <p className="section-intro">云端不保存此密钥。首次上传后从当前设备导出；在新设备登录同一账户后导入，才能解密数据。</p>
      {revealedKey && <div className="recovery-value"><code>{revealedKey}</code><button className="secondary" onClick={copyRecoveryKey}><Copy size={14} />复制</button></div>}
      <div className="form-grid single-column"><label><span>从其他设备导入恢复密钥</span><input type="password" value={recoveryKey} onChange={(event) => setRecoveryKey(event.target.value)} placeholder="zhiheng-sync-v1:…" /></label></div>
      <div className="form-actions"><button className="secondary" onClick={revealRecoveryKey} disabled={busy !== "" || !status.signedIn || !status.hasRecoveryKey}>显示本机恢复密钥</button><button className="primary" onClick={importRecoveryKey} disabled={busy !== "" || !status.signedIn || !recoveryKey}>保存导入密钥</button></div>
    </section>

    {busy && <div className="connection-success"><LoaderCircle size={16} className="spin" /><span><strong>正在处理</strong>同步期间请不要关闭应用</span></div>}
    {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}
    {result && <div className="connection-success"><Check size={16} /><span><strong>操作完成</strong>{result}</span></div>}
  </div>;
}

function ModelSettings({ model, onUpdate, flash }: { model: ModelConfig; onUpdate: (m: ModelConfig) => void; flash: (s: string) => void }) {
  const [baseUrl, setBaseUrl] = useState(model.baseUrl);
  const [modelName, setModelName] = useState(model.model);
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [connectionResult, setConnectionResult] = useState("");
  const [error, setError] = useState("");

  const persist = async () => {
    setSaving(true); setError(""); setConnectionResult("");
    try {
      const next = await saveModelConfig({ provider: "openai-compatible", baseUrl, model: modelName, apiKey: apiKey || undefined });
      onUpdate(next); setApiKey(""); flash("模型配置已安全保存");
    } catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  const testConnection = async () => {
    setTesting(true); setError(""); setConnectionResult("");
    try {
      const result = await testModelConnection();
      setConnectionResult(`${result.model} · ${result.latencyMs} ms`);
    } catch (nextError) { setError(String(nextError)); } finally { setTesting(false); }
  };

  const clearKey = async () => {
    if (!window.confirm("确认从系统钥匙串中移除模型 API Key？")) return;
    setSaving(true); setError(""); setConnectionResult("");
    try { onUpdate(await deleteModelKey()); flash("模型密钥已从系统钥匙串移除"); }
    catch (nextError) { setError(String(nextError)); } finally { setSaving(false); }
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="模型与隐私" title="模型可以替换，方法论保持稳定" description="AI 提供商通过统一接口接入；投资数据只在发起分析时按需发送。" />
      <section className="panel form-panel">
        <div className="panel-title"><div><span>OpenAI-compatible</span><h2>模型连接</h2></div><div className={`status-dot ${model.hasApiKey ? "connected" : ""}`}>{model.hasApiKey ? "已配置" : "未配置"}</div></div>
        <div className="form-grid single-column">
          <label><span>API Base URL</span><input value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://api.openai.com/v1" /></label>
          <label><span>模型名称</span><input value={modelName} onChange={(e) => setModelName(e.target.value)} placeholder="gpt-4.1-mini" /></label>
          <label><span>API Key</span><div className="secure-input"><KeyRound size={16} /><input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder={model.hasApiKey ? "已保存在系统钥匙串；留空则不修改" : "输入模型供应商密钥"} /></div></label>
        </div>
        <div className="privacy-note"><LockKeyhole size={18} /><div><strong>密钥与业务数据分离</strong><p>密钥由操作系统钥匙串托管；本地 SQLite 数据库只保存提供商、地址和模型名称。</p></div></div>
        {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}
        {connectionResult && <div className="connection-success"><Check size={16} /><span><strong>连接成功</strong>{connectionResult}</span></div>}
        <div className="form-actions">
          <div className="key-actions">{model.hasApiKey && <button className="danger-text" onClick={clearKey} disabled={saving}><Trash2 size={14} />移除密钥</button>}<button className="secondary" onClick={testConnection} disabled={testing || !model.hasApiKey}>{testing ? <LoaderCircle size={15} className="spin" /> : <Bot size={15} />}测试已保存连接</button></div>
          <button className="primary" onClick={persist} disabled={saving || !baseUrl || !modelName}><Save size={16} />保存配置</button>
        </div>
      </section>
      <section className="architecture-grid">
        <article><span>01</span><strong>确定性规则层</strong><p>现金流、集中度、期限错配等风险无需调用模型。</p></article>
        <article><span>02</span><strong>上下文构建层</strong><p>只选择完成当前任务所需的本地数据。</p></article>
        <article><span>03</span><strong>可替换编排层</strong><p>记忆、检索、探索和反思都是独立阶段。</p></article>
      </section>
    </div>
  );
}

export default App;
