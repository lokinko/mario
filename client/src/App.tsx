import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowRight,
  Bot,
  BrainCircuit,
  Check,
  ChevronRight,
  CircleDollarSign,
  Compass,
  Database,
  FilePenLine,
  KeyRound,
  LayoutDashboard,
  LoaderCircle,
  LockKeyhole,
  Plus,
  Save,
  Settings2,
  ShieldCheck,
  Sparkles,
  Target,
  WalletCards,
} from "lucide-react";
import {
  getModelConfig,
  getSnapshot,
  runAnalysis,
  saveDecision,
  saveGoal,
  saveHolding,
  saveModelConfig,
  saveProfile,
} from "./api";
import type {
  AnalysisResult,
  DecisionEntry,
  FinancialProfile,
  Goal,
  Holding,
  ModelConfig,
  Snapshot,
} from "./types";

type View = "dashboard" | "foundation" | "decision" | "advisor" | "settings";

const money = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  maximumFractionDigits: 0,
});

const nav = [
  { id: "dashboard" as const, label: "决策总览", icon: LayoutDashboard },
  { id: "foundation" as const, label: "财务底座", icon: WalletCards },
  { id: "decision" as const, label: "决策日志", icon: FilePenLine },
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

function App() {
  const [view, setView] = useState<View>("dashboard");
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [model, setModel] = useState<ModelConfig | null>(null);
  const [loading, setLoading] = useState(true);
  const [startupError, setStartupError] = useState("");
  const [notice, setNotice] = useState("");

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

  const flash = (message: string) => {
    setNotice(message);
    window.setTimeout(() => setNotice(""), 2400);
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
        <span>客户端没有连接到 127.0.0.1:4217。你的数据没有丢失。</span>
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
            <button key={item.id} className={view === item.id ? "active" : ""} onClick={() => setView(item.id)}>
              <item.icon size={18} />{item.label}
            </button>
          ))}
        </nav>

        <div className="sidebar-spacer" />
        <div className="privacy-card">
          <LockKeyhole size={18} />
          <div><strong>本地优先</strong><span>财务档案存储在此设备</span></div>
        </div>
        <button className={`settings-link ${view === "settings" ? "active" : ""}`} onClick={() => setView("settings")}>
          <Settings2 size={18} /> 模型与隐私
        </button>
      </aside>

      <main>
        {notice && <div className="toast"><Check size={16} />{notice}</div>}
        {view === "dashboard" && <Dashboard snapshot={snapshot} model={model} navigate={setView} />}
        {view === "foundation" && <Foundation snapshot={snapshot} onUpdate={setSnapshot} flash={flash} />}
        {view === "decision" && <DecisionJournal flash={flash} />}
        {view === "advisor" && <Advisor model={model} navigate={setView} />}
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

      <section className="method-strip">
        <div><p className="eyebrow">知衡决策闭环</p><h2>每一次判断，都留下可复盘的证据</h2></div>
        {["财务底座", "目标配置", "独立研究", "仓位决策", "复盘校准"].map((step, index) => (
          <div className="method-step" key={step}><span>0{index + 1}</span><strong>{step}</strong>{index < 4 && <ArrowRight size={15} />}</div>
        ))}
      </section>
    </div>
  );
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
  const [holding, setHolding] = useState<Omit<Holding, "id">>({ symbol: "", name: "", assetClass: "基金", marketValue: 0, costBasis: 0, currency: "CNY" });
  const [goal, setGoal] = useState<Omit<Goal, "id">>({ name: "", targetAmount: 0, targetDate: "", priority: "重要" });
  const [saving, setSaving] = useState(false);

  const updateNumber = (key: keyof FinancialProfile, value: string) => setProfile({ ...profile, [key]: Number(value) });

  const persistProfile = async () => {
    setSaving(true);
    try { onUpdate(await saveProfile(profile)); flash("财务档案已保存在本机"); } finally { setSaving(false); }
  };

  const addHolding = async () => {
    if (!holding.name || holding.marketValue <= 0) return;
    setSaving(true);
    try {
      onUpdate(await saveHolding(holding));
      setHolding({ symbol: "", name: "", assetClass: "基金", marketValue: 0, costBasis: 0, currency: "CNY" });
      flash("资产已加入组合");
    } finally { setSaving(false); }
  };

  const addGoal = async () => {
    if (!goal.name || goal.targetAmount <= 0 || !goal.targetDate) return;
    setSaving(true);
    try {
      onUpdate(await saveGoal(goal));
      setGoal({ name: "", targetAmount: 0, targetDate: "", priority: "重要" });
      flash("投资目标已保存");
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
        <div className="panel-title"><div><span>目标账户</span><h2>给资金一个明确任务</h2></div><Target size={21} className="muted-icon" /></div>
        {snapshot.goals.length > 0 && <div className="goal-list">{snapshot.goals.map((item) => <div key={item.id}><span>{item.priority}</span><strong>{item.name}</strong><em>{money.format(item.targetAmount)} · {item.targetDate}</em></div>)}</div>}
        <div className="form-grid compact-grid">
          <label><span>目标名称</span><input value={goal.name} onChange={(e) => setGoal({ ...goal, name: e.target.value })} placeholder="例如：长期养老账户" /></label>
          <NumberField label="目标金额" value={goal.targetAmount} onChange={(v) => setGoal({ ...goal, targetAmount: Number(v) })} prefix="¥" />
          <label><span>目标日期</span><input type="date" value={goal.targetDate} onChange={(e) => setGoal({ ...goal, targetDate: e.target.value })} /></label>
          <label><span>目标优先级</span><select value={goal.priority} onChange={(e) => setGoal({ ...goal, priority: e.target.value as Goal["priority"] })}><option>刚性</option><option>重要</option><option>弹性</option></select></label>
        </div>
        <div className="form-actions"><p>目标决定期限，期限决定可以承担的波动。</p><button className="secondary" onClick={addGoal} disabled={saving || !goal.name}><Plus size={16} />添加目标</button></div>
      </section>

      <section className="panel form-panel">
        <div className="panel-title"><div><span>组合输入</span><h2>添加一项资产</h2></div><CircleDollarSign size={21} className="muted-icon" /></div>
        <div className="form-grid compact-grid">
          <label><span>资产名称</span><input value={holding.name} onChange={(e) => setHolding({ ...holding, name: e.target.value })} placeholder="例如：宽基指数基金" /></label>
          <label><span>代码（可选）</span><input value={holding.symbol} onChange={(e) => setHolding({ ...holding, symbol: e.target.value })} placeholder="例如：000300" /></label>
          <label><span>资产类别</span><select value={holding.assetClass} onChange={(e) => setHolding({ ...holding, assetClass: e.target.value as Holding["assetClass"] })}>{["现金", "债券", "股票", "基金", "黄金", "其他"].map((v) => <option key={v}>{v}</option>)}</select></label>
          <NumberField label="当前市值" value={holding.marketValue} onChange={(v) => setHolding({ ...holding, marketValue: Number(v) })} prefix="¥" />
          <NumberField label="累计成本" value={holding.costBasis} onChange={(v) => setHolding({ ...holding, costBasis: Number(v) })} prefix="¥" />
        </div>
        <div className="form-actions"><span /><button className="secondary" onClick={addHolding} disabled={saving || !holding.name}><Plus size={16} />加入组合</button></div>
      </section>
    </div>
  );
}

function NumberField({ label, value, onChange, prefix, suffix }: { label: string; value: number; onChange: (v: string) => void; prefix?: string; suffix?: string }) {
  return <label><span>{label}</span><div className="input-affix">{prefix && <i>{prefix}</i>}<input type="number" value={value || ""} onChange={(e) => onChange(e.target.value)} />{suffix && <i>{suffix}</i>}</div></label>;
}

const emptyDecision: DecisionEntry = {
  assetName: "", thesis: "", counterThesis: "", expectedReturnPct: 0, downsidePct: 0,
  confidencePct: 50, positionPct: 0, invalidation: "", reviewDate: "",
};

function DecisionJournal({ flash }: { flash: (s: string) => void }) {
  const [entry, setEntry] = useState(emptyDecision);
  const [saving, setSaving] = useState(false);
  const expectedValue = entry.confidencePct / 100 * entry.expectedReturnPct - (1 - entry.confidencePct / 100) * Math.abs(entry.downsidePct);

  const persist = async () => {
    if (!entry.assetName || !entry.thesis || !entry.counterThesis || !entry.invalidation) return;
    setSaving(true);
    try { await saveDecision(entry); flash("决策快照已冻结，可用于未来复盘"); setEntry(emptyDecision); } finally { setSaving(false); }
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="方法论 · 决策层" title="先写下来，再按下买入" description="记录当时真正知道的事，避免用事后结果改写记忆。" />
      <section className="panel form-panel decision-card">
        <div className="panel-title"><div><span>投资决策卡</span><h2>把观点变成可证伪的假设</h2></div><FilePenLine size={21} className="muted-icon" /></div>
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
        <div className="form-actions"><p>必填：投资对象、正反逻辑与证伪条件</p><button className="primary" onClick={persist} disabled={saving}><Save size={16} />冻结决策快照</button></div>
      </section>
    </div>
  );
}

function Advisor({ model, navigate }: { model: ModelConfig; navigate: (v: View) => void }) {
  const [question, setQuestion] = useState("请基于我的财务目标和当前组合，指出最需要优先处理的风险，并给出不依赖市场预测的改进方案。");
  const [deep, setDeep] = useState(true);
  const [memory, setMemory] = useState(true);
  const [reflection, setReflection] = useState(true);
  const [alternatives, setAlternatives] = useState(true);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [error, setError] = useState("");

  const analyze = async () => {
    setRunning(true); setError(""); setResult(null);
    try {
      setResult(await runAnalysis({ question, workflow: deep ? "deep" : "quick", useMemory: memory, reflect: reflection, exploreAlternatives: alternatives }));
    } catch (e) { setError(String(e)); } finally { setRunning(false); }
  };

  return (
    <div className="page narrow">
      <PageHeader eyebrow="AI 原生分析" title="研究室，而不是荐股机" description="规则引擎先处理确定性风险，大模型负责理解、比较、反驳与解释。" action={<div className={`model-pill ${model.hasApiKey ? "ready" : ""}`}><Bot size={15} />{model.hasApiKey ? model.model : "尚未配置模型"}</div>} />
      {!model.hasApiKey && <div className="setup-banner"><KeyRound size={20} /><div><strong>配置自己的模型密钥</strong><p>密钥保存到系统钥匙串，不写入投资数据库。</p></div><button className="secondary" onClick={() => navigate("settings")}>立即配置</button></div>}
      <section className="panel advisor-panel">
        <label className="question-box"><span>这次希望解决什么问题？</span><textarea value={question} onChange={(e) => setQuestion(e.target.value)} /></label>
        <div className="workflow-options">
          <Toggle icon={<BrainCircuit size={17} />} title="深度编排" detail="构建计划并分阶段分析" checked={deep} onChange={setDeep} />
          <Toggle icon={<Database size={17} />} title="本地记忆" detail="检索相关相关历史决策" checked={memory} onChange={setMemory} />
          <Toggle icon={<ShieldCheck size={17} />} title="纠错反思" detail="独立检查遗漏和过度自信" checked={reflection} onChange={setReflection} />
          <Toggle icon={<Sparkles size={17} />} title="多方案探索" detail="比较至少两条可行路径" checked={alternatives} onChange={setAlternatives} />
        </div>
        <button className="primary analyze-button" onClick={analyze} disabled={running || !question.trim() || !model.hasApiKey}>
          {running ? <><LoaderCircle size={17} className="spin" />正在执行分析链路…</> : <><Sparkles size={17} />开始结构化分析</>}
        </button>
      </section>

      {error && <div className="error-box"><AlertTriangle size={18} />{error}</div>}
      {result && <section className="panel result-panel"><div className="result-meta">{result.stages.map((stage) => <span key={stage}><Check size={13} />{stage}</span>)}</div><div className="answer">{result.answer}</div><p className="disclaimer">{result.disclaimer}</p></section>}
    </div>
  );
}

function Toggle({ icon, title, detail, checked, onChange }: { icon: React.ReactNode; title: string; detail: string; checked: boolean; onChange: (v: boolean) => void }) {
  return <button className={`toggle-card ${checked ? "selected" : ""}`} onClick={() => onChange(!checked)}><span className="toggle-icon">{icon}</span><div><strong>{title}</strong><small>{detail}</small></div><i>{checked && <Check size={13} />}</i></button>;
}

function ModelSettings({ model, onUpdate, flash }: { model: ModelConfig; onUpdate: (m: ModelConfig) => void; flash: (s: string) => void }) {
  const [baseUrl, setBaseUrl] = useState(model.baseUrl);
  const [modelName, setModelName] = useState(model.model);
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);

  const persist = async () => {
    setSaving(true);
    try {
      const next = await saveModelConfig({ provider: "openai-compatible", baseUrl, model: modelName, apiKey: apiKey || undefined });
      onUpdate(next); setApiKey(""); flash("模型配置已安全保存");
    } finally { setSaving(false); }
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
        <div className="form-actions"><span /><button className="primary" onClick={persist} disabled={saving || !baseUrl || !modelName}><Save size={16} />保存配置</button></div>
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
