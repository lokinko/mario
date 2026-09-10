import { useRef, useState } from "react";
import {
  CircleDollarSign,
  Cloud,
  Database,
  Edit3,
  LoaderCircle,
  Plus,
  Save,
  ShieldCheck,
  Target,
  Trash2,
} from "lucide-react";
import {
  applyVerifiedHoldingValuation,
  deleteHolding,
  deleteGoal,
  getFxRate,
  saveGoal,
  saveHolding,
  saveProfile,
  updateHolding,
  updateGoal,
} from "../../api";
import type {
  FinancialProfile,
  FxRateQuote,
  Goal,
  Holding,
  HoldingValuationEvidence,
  Snapshot,
} from "../../types";
import { PageHeader } from "../../components/PageHeader";
import type { FoundationSection } from "../../app/navigation";
import { NumberField } from "../../components/NumberField";
import { localDateValue } from "../../lib/dates";
import { emptyProfile, emptyHolding, emptyGoal } from "./forms";
import {
  normalizedQuoteRate,
  fxSourceLabel,
  holdingValueInBase,
  ecbFxMethodologyUrl,
} from "./valuation";
import { formatMoney } from "../../lib/format";

export function Foundation({
  snapshot,
  onUpdate,
  flash,
  initialSection = "holdings",
}: {
  snapshot: Snapshot;
  onUpdate: (s: Snapshot) => void;
  flash: (s: string) => void;
  initialSection?: FoundationSection;
}) {
  const [section, setSection] = useState<string>(initialSection);
  const [profile, setProfile] = useState(snapshot.profile ?? emptyProfile);
  const [holding, setHolding] = useState<Omit<Holding, "id">>(() =>
    emptyHolding(snapshot.profile.baseCurrency),
  );
  const [editingHoldingId, setEditingHoldingId] = useState<string | null>(null);
  const [holdingDetailsOpen, setHoldingDetailsOpen] = useState(false);
  const holdingNameRef = useRef<HTMLInputElement>(null);
  const [goal, setGoal] = useState<Omit<Goal, "id">>(emptyGoal);
  const [editingGoalId, setEditingGoalId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [holdingFxQuote, setHoldingFxQuote] = useState<FxRateQuote | null>(
    null,
  );
  const [holdingFxLoading, setHoldingFxLoading] = useState(false);
  const [holdingFxError, setHoldingFxError] = useState("");
  const [valuationQuantity, setValuationQuantity] = useState(0);
  const [valuationLoading, setValuationLoading] = useState(false);
  const [valuationError, setValuationError] = useState("");
  const holdingBusy = saving || holdingFxLoading || valuationLoading;
  const canSaveHolding =
    !holdingBusy &&
    Boolean(holding.name.trim()) &&
    Number.isFinite(holding.marketValue) &&
    holding.marketValue > 0 &&
    Boolean(holding.valuationDate) &&
    holding.valuationDate <= localDateValue(new Date()) &&
    (holding.currency === profile.baseCurrency ||
      (Number.isFinite(holding.fxRateToBase) &&
        (holding.fxRateToBase ?? 0) > 0));

  const updateNumber = (key: keyof FinancialProfile, value: string) =>
    setProfile({ ...profile, [key]: Number(value) });

  const persistProfile = async () => {
    setError("");
    setSaving(true);
    try {
      onUpdate(await saveProfile(profile));
      flash("财务档案已保存在本机");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const persistHolding = async () => {
    setError("");
    if (!canSaveHolding) return;
    setSaving(true);
    try {
      const next = editingHoldingId
        ? await updateHolding(editingHoldingId, {
            ...holding,
            name: holding.name.trim(),
          })
        : await saveHolding({ ...holding, name: holding.name.trim() });
      onUpdate(next);
      setHolding(
        editingHoldingId
          ? emptyHolding(profile.baseCurrency)
          : {
              ...emptyHolding(holding.currency),
              assetClass: holding.assetClass,
              valuationDate: holding.valuationDate,
              fxRateToBase: holding.fxRateToBase,
              fxRateSource: holding.fxRateSource,
              fxRateObservedOn: holding.fxRateObservedOn,
            },
      );
      setHoldingDetailsOpen(false);
      setHoldingFxQuote(null);
      setHoldingFxError("");
      setValuationQuantity(0);
      setValuationError("");
      setEditingHoldingId(null);
      flash(editingHoldingId ? "资产信息已更新" : "资产已加入组合");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const editHolding = (item: Holding) => {
    const { id, ...values } = item;
    setHolding(values);
    setHoldingDetailsOpen(true);
    holdingNameRef.current?.focus();
    holdingNameRef.current?.scrollIntoView?.({
      block: "center",
      behavior: "smooth",
    });
    setEditingHoldingId(id);
    setHoldingFxQuote(null);
    setHoldingFxError("");
    setValuationQuantity(
      snapshot.holdingValuations.find((value) => value.holdingId === id)
        ?.quantity ?? 0,
    );
    setValuationError("");
  };

  const lookupHoldingFx = async () => {
    setHoldingFxLoading(true);
    setHoldingFxError("");
    try {
      const quote = await getFxRate(
        holding.currency,
        profile.baseCurrency,
        holding.valuationDate,
      );
      setHolding((value) => ({
        ...value,
        fxRateToBase: normalizedQuoteRate(quote.rate),
        fxRateSource: quote.providerCode,
        fxRateObservedOn: quote.observedOn,
      }));
      setHoldingFxQuote(quote);
    } catch (nextError) {
      setHoldingFxQuote(null);
      setHoldingFxError(String(nextError));
    } finally {
      setHoldingFxLoading(false);
    }
  };

  const applyMarketValuation = async () => {
    if (
      !editingHoldingId ||
      !holding.symbol.trim() ||
      valuationQuantity <= 0 ||
      !holding.valuationDate
    )
      return;
    setValuationLoading(true);
    setValuationError("");
    try {
      const next = await applyVerifiedHoldingValuation(
        editingHoldingId,
        holding.symbol,
        valuationQuantity,
        holding.valuationDate,
      );
      onUpdate(next);
      const updated = next.holdings.find(
        (value) => value.id === editingHoldingId,
      );
      if (updated) {
        const { id: _id, ...values } = updated;
        setHolding(values);
      }
      setHoldingFxQuote(null);
      flash("已冻结数量、日收盘价和来源；外币汇率如失效需重新查询");
    } catch (nextError) {
      setValuationError(String(nextError));
    } finally {
      setValuationLoading(false);
    }
  };

  const removeHolding = async (item: Holding) => {
    setError("");
    if (!window.confirm(`确认删除“${item.name}”？相关决策日志不会被删除。`))
      return;
    setSaving(true);
    try {
      onUpdate(await deleteHolding(item.id));
      if (editingHoldingId === item.id) {
        setEditingHoldingId(null);
        setHolding(emptyHolding(profile.baseCurrency));
      }
      flash("资产已从组合删除");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const persistGoal = async () => {
    setError("");
    if (!goal.name || goal.targetAmount <= 0 || !goal.targetDate) return;
    setSaving(true);
    try {
      const next = editingGoalId
        ? await updateGoal(editingGoalId, goal)
        : await saveGoal(goal);
      onUpdate(next);
      setGoal(emptyGoal);
      setEditingGoalId(null);
      flash(editingGoalId ? "投资目标已更新" : "投资目标已保存");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const editGoal = (item: Goal) => {
    const { id, ...values } = item;
    setGoal(values);
    setEditingGoalId(id);
  };

  const removeGoal = async (item: Goal) => {
    setError("");
    if (!window.confirm(`确认删除目标“${item.name}”？`)) return;
    setSaving(true);
    try {
      onUpdate(await deleteGoal(item.id));
      if (editingGoalId === item.id) {
        setEditingGoalId(null);
        setGoal(emptyGoal);
      }
      flash("投资目标已删除");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="page narrow">
      <PageHeader title="财务底座" />
      <div className="section-switcher" role="group" aria-label="财务分类">
        {[
          ["holdings", "持仓", snapshot.holdings.length],
          ["profile", "收支与风险", null],
          ["goals", "目标", snapshot.goals.length],
        ].map(([id, label, count]) => (
          <button
            key={id}
            type="button"
            aria-pressed={section === id}
            onClick={() => setSection(String(id))}
          >
            {label}
            {count !== null && <span>{count}</span>}
          </button>
        ))}
      </div>
      {error && (
        <div className="error-box" role="alert">
          {error}。输入内容已保留；请核实当前记录后再重试。
        </div>
      )}
      <section className="panel form-panel" hidden={section !== "holdings"}>
        <div className="panel-title">
          <div>
            <h2>{editingHoldingId ? "修改资产" : "管理资产组合"}</h2>
          </div>
        </div>
        <p className="holding-hint">名称、市值必填。</p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void persistHolding();
          }}
        >
          <fieldset
            className="holding-fields"
            disabled={saving || holdingFxLoading || valuationLoading}
          >
            <div className="form-grid compact-grid">
              <label>
                <span>资产名称</span>
                <input
                  ref={holdingNameRef}
                  required
                  value={holding.name}
                  onChange={(e) =>
                    setHolding({ ...holding, name: e.target.value })
                  }
                  placeholder="例如：宽基指数基金"
                />
              </label>
              <NumberField
                label={`当前市值（${holding.currency}）`}
                value={holding.marketValue}
                onChange={(v) =>
                  setHolding({ ...holding, marketValue: Number(v) })
                }
                prefix={holding.currency}
              />
              <label>
                <span>资产类别</span>
                <select
                  value={holding.assetClass}
                  onChange={(e) =>
                    setHolding({
                      ...holding,
                      assetClass: e.target.value as Holding["assetClass"],
                    })
                  }
                >
                  {["现金", "债券", "股票", "基金", "黄金", "其他"].map((v) => (
                    <option key={v}>{v}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>持仓币种</span>
                <select
                  value={holding.currency}
                  onChange={(e) => {
                    setHolding({
                      ...holding,
                      currency: e.target.value,
                      fxRateToBase: null,
                      fxRateSource: "",
                      fxRateObservedOn: "",
                    });
                    setHoldingFxQuote(null);
                    setHoldingFxError("");
                  }}
                >
                  {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map((value) => (
                    <option key={value}>{value}</option>
                  ))}
                </select>
              </label>
              <label>
                <span>估值日期</span>
                <input
                  type="date"
                  max={localDateValue(new Date())}
                  value={holding.valuationDate}
                  onChange={(e) => {
                    setHolding({
                      ...holding,
                      valuationDate: e.target.value,
                      fxRateToBase: null,
                      fxRateSource: "",
                      fxRateObservedOn: "",
                    });
                    setHoldingFxQuote(null);
                    setHoldingFxError("");
                  }}
                />
                <small>所有持仓请使用同一估值日</small>
              </label>
            </div>
            <details
              className="holding-details"
              open={holdingDetailsOpen}
              onToggle={(event) =>
                setHoldingDetailsOpen(event.currentTarget.open)
              }
            >
              <summary>更多信息</summary>
              <div className="form-grid compact-grid">
                <label>
                  <span>代码（可选）</span>
                  <input
                    value={holding.symbol}
                    onChange={(e) =>
                      setHolding({ ...holding, symbol: e.target.value })
                    }
                    placeholder="例如：000300"
                  />
                </label>
                <NumberField
                  label={`累计成本（${holding.currency}）`}
                  value={holding.costBasis}
                  onChange={(v) =>
                    setHolding({ ...holding, costBasis: Number(v) })
                  }
                  prefix={holding.currency}
                />
                <NumberField
                  label="目标权重"
                  value={holding.targetPct}
                  onChange={(v) =>
                    setHolding({ ...holding, targetPct: Number(v) })
                  }
                  suffix="%"
                />
                {editingHoldingId && (
                  <>
                    <NumberField
                      label="估值日持仓数量"
                      value={valuationQuantity}
                      onChange={(value) => setValuationQuantity(Number(value))}
                      suffix="份 / 股"
                    />
                    <div className="price-lookup">
                      <button
                        type="button"
                        className="secondary"
                        disabled={
                          valuationLoading ||
                          !holding.symbol.trim() ||
                          valuationQuantity <= 0 ||
                          !holding.valuationDate
                        }
                        onClick={applyMarketValuation}
                      >
                        {valuationLoading ? (
                          <LoaderCircle className="spin" size={15} />
                        ) : (
                          <Cloud size={15} />
                        )}
                        查询并采用日收盘价
                      </button>
                      {valuationError && (
                        <small className="fx-error">{valuationError}</small>
                      )}
                      {snapshot.holdingValuations.find(
                        (value) => value.holdingId === editingHoldingId,
                      ) &&
                        ((valuation) => (
                          <p>
                            <strong>
                              {valuation.quantity} × {valuation.unitPrice}{" "}
                              {valuation.currency} ={" "}
                              {formatMoney(
                                valuation.marketValue,
                                valuation.currency,
                              )}
                            </strong>
                            <span>
                              {valuation.exchange ||
                                valuation.micCode ||
                                "交易所未标注"}{" "}
                              · {valuation.observedOn}
                              {valuation.stalenessDays
                                ? `（回退 ${valuation.stalenessDays} 天）`
                                : ""}{" "}
                              · 未复权日收盘价
                            </span>
                            <a
                              href={valuation.sourceUrl}
                              target="_blank"
                              rel="noreferrer"
                            >
                              核对请求来源
                            </a>
                          </p>
                        ))(
                          snapshot.holdingValuations.find(
                            (value) => value.holdingId === editingHoldingId,
                          ) as HoldingValuationEvidence,
                        )}
                    </div>
                  </>
                )}
              </div>
              <p className="holding-hint">
                成本和目标权重可以稍后补充。查询证券收盘价需先保存持仓，再点击编辑。
              </p>
            </details>
            <div className="form-grid compact-grid">
              {holding.currency !== profile.baseCurrency && (
                <>
                  <NumberField
                    label={`折算汇率（1 ${holding.currency} = ? ${profile.baseCurrency}）`}
                    value={holding.fxRateToBase ?? 0}
                    onChange={(v) => {
                      setHolding({
                        ...holding,
                        fxRateToBase: v ? Number(v) : null,
                        fxRateSource: "",
                        fxRateObservedOn: "",
                      });
                      setHoldingFxQuote(null);
                    }}
                    suffix={profile.baseCurrency}
                  />
                  <div className="fx-lookup">
                    <button
                      type="button"
                      className="secondary"
                      disabled={holdingFxLoading || !holding.valuationDate}
                      onClick={lookupHoldingFx}
                    >
                      {holdingFxLoading ? (
                        <LoaderCircle className="spin" size={15} />
                      ) : (
                        <Cloud size={15} />
                      )}
                      查询 ECB 当日参考汇率
                    </button>
                    {holdingFxError && (
                      <small className="fx-error">{holdingFxError}</small>
                    )}
                    {holding.fxRateSource && (
                      <small>
                        已采用 {fxSourceLabel(holding.fxRateSource)} · 观察日{" "}
                        {holding.fxRateObservedOn}
                      </small>
                    )}
                    {holdingFxQuote && (
                      <p>
                        {holdingFxQuote.stalenessDays
                          ? `非工作日，使用此前 ${holdingFxQuote.stalenessDays} 天的共同观察值。`
                          : "已取得当日共同观察值。"}
                        <a
                          href={holdingFxQuote.sourceUrl}
                          target="_blank"
                          rel="noreferrer"
                        >
                          核对原始数据
                        </a>
                      </p>
                    )}
                    {holding.fxRateSource === "ecb_reference" &&
                      !holdingFxQuote && (
                        <a
                          className="fx-method-link"
                          href={ecbFxMethodologyUrl}
                          target="_blank"
                          rel="noreferrer"
                        >
                          查看 ECB 参考汇率方法
                        </a>
                      )}
                  </div>
                </>
              )}
            </div>
            <div className="form-actions">
              {editingHoldingId ? (
                <button
                  type="button"
                  className="text-button"
                  onClick={() => {
                    setHoldingDetailsOpen(false);
                    setEditingHoldingId(null);
                    setHolding(emptyHolding(profile.baseCurrency));
                    setHoldingFxQuote(null);
                    setHoldingFxError("");
                    setValuationQuantity(0);
                    setValuationError("");
                  }}
                >
                  取消修改
                </button>
              ) : (
                <p>连续添加会沿用类别、币种和日期</p>
              )}
              <button
                className="primary"
                type="submit"
                disabled={!canSaveHolding}
              >
                {editingHoldingId ? <Save size={16} /> : <Plus size={16} />}
                {editingHoldingId ? "保存修改" : "加入组合"}
              </button>
            </div>
          </fieldset>
        </form>
        {snapshot.holdings.length > 0 && (
          <div className="holding-list">
            <div className="holding-head">
              <span>资产</span>
              <span>类别</span>
              <span>市值</span>
              <span>目标权重</span>
              <span>账面变化</span>
              <span />
            </div>
            {snapshot.holdings.map((item) => {
              const pnlPct =
                item.costBasis > 0
                  ? ((item.marketValue - item.costBasis) / item.costBasis) * 100
                  : 0;
              const valuation = snapshot.holdingValuations.find(
                (value) => value.holdingId === item.id,
              );
              return (
                <div
                  className={editingHoldingId === item.id ? "editing" : ""}
                  key={item.id}
                >
                  <strong>
                    {item.name}
                    {item.symbol && <small>{item.symbol}</small>}
                    {valuation && (
                      <small className="verified-source">
                        已核验 · {valuation.providerName} ·{" "}
                        {valuation.observedOn}
                      </small>
                    )}
                  </strong>
                  <span>
                    {item.assetClass}
                    <small>
                      {item.currency}
                      {item.currency !== profile.baseCurrency &&
                      item.fxRateToBase
                        ? ` · 汇率 ${item.fxRateToBase}`
                        : ""}
                    </small>
                    {item.fxRateSource && (
                      <small>
                        {fxSourceLabel(item.fxRateSource)} ·{" "}
                        {item.fxRateObservedOn}
                      </small>
                    )}
                  </span>
                  <span>
                    {formatMoney(item.marketValue, item.currency)}
                    <small>
                      {item.currency !== profile.baseCurrency &&
                      item.fxRateToBase
                        ? `折合 ${formatMoney(holdingValueInBase(item, profile.baseCurrency), profile.baseCurrency)} · `
                        : ""}
                      {item.valuationDate || "待补估值日"}
                    </small>
                  </span>
                  <span>
                    {item.targetPct
                      ? `${item.targetPct.toFixed(1)}%`
                      : "未设置"}
                  </span>
                  <span
                    className={
                      item.costBasis > 0 ? (pnlPct >= 0 ? "gain" : "loss") : ""
                    }
                  >
                    {item.costBasis > 0
                      ? `${pnlPct >= 0 ? "+" : ""}${pnlPct.toFixed(1)}%`
                      : "待补成本"}
                  </span>
                  <span className="row-actions">
                    <button
                      type="button"
                      disabled={saving || holdingFxLoading || valuationLoading}
                      aria-label="编辑资产"
                      onClick={() => editHolding(item)}
                    >
                      <Edit3 size={14} />
                    </button>
                    <button
                      type="button"
                      disabled={saving || holdingFxLoading || valuationLoading}
                      aria-label="删除资产"
                      onClick={() => removeHolding(item)}
                    >
                      <Trash2 size={14} />
                    </button>
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </section>

      <section className="panel form-panel" hidden={section !== "profile"}>
        <div className="panel-title">
          <div>
            <h2>收支与风险</h2>
          </div>
        </div>
        <div className="form-grid">
          <label>
            <span>基准币种</span>
            <select
              value={profile.baseCurrency}
              onChange={(e) =>
                setProfile({ ...profile, baseCurrency: e.target.value })
              }
            >
              {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map((value) => (
                <option key={value}>{value}</option>
              ))}
            </select>
            <small>
              财务、目标和组合汇总统一使用此币种；切换不会自动换算已有金额。
            </small>
          </label>
          <NumberField
            label="月收入"
            value={profile.monthlyIncome}
            onChange={(v) => updateNumber("monthlyIncome", v)}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="月支出"
            value={profile.monthlyExpense}
            onChange={(v) => updateNumber("monthlyExpense", v)}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="应急资金"
            value={profile.emergencyFund}
            onChange={(v) => updateNumber("emergencyFund", v)}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="负债余额"
            value={profile.liabilities}
            onChange={(v) => updateNumber("liabilities", v)}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="可投资资产"
            value={profile.investableAssets}
            onChange={(v) => updateNumber("investableAssets", v)}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="投资期限"
            value={profile.horizonYears}
            onChange={(v) => updateNumber("horizonYears", v)}
            suffix="年"
          />
          <NumberField
            label="可承受最大回撤"
            value={profile.maxDrawdownPct}
            onChange={(v) => updateNumber("maxDrawdownPct", v)}
            suffix="%"
          />
          <label>
            <span>风险倾向</span>
            <select
              value={profile.riskLevel}
              onChange={(e) =>
                setProfile({
                  ...profile,
                  riskLevel: e.target.value as FinancialProfile["riskLevel"],
                })
              }
            >
              <option>保守</option>
              <option>稳健</option>
              <option>均衡</option>
              <option>进取</option>
            </select>
          </label>
        </div>
        <div className="form-actions">
          <p>
            <ShieldCheck size={16} />
            按实际收支评估风险容量
          </p>
          <button
            className="primary"
            onClick={persistProfile}
            disabled={saving}
          >
            <Save size={16} />
            保存并检查
          </button>
        </div>
      </section>

      <section className="panel form-panel" hidden={section !== "goals"}>
        <div className="panel-title">
          <div>
            <h2>{editingGoalId ? "修改目标计划" : "投资目标"}</h2>
          </div>
        </div>
        {snapshot.goals.length > 0 && (
          <div className="goal-list">
            {snapshot.goals.map((item) => {
              const projection = snapshot.plan.goalProjections.find(
                (value) => value.goalId === item.id,
              );
              return (
                <div
                  className={editingGoalId === item.id ? "editing" : ""}
                  key={item.id}
                >
                  <span>{item.priority}</span>
                  <strong>
                    {item.name}
                    <small>
                      已投入{" "}
                      {formatMoney(item.currentAmount, profile.baseCurrency)} ·
                      每月{" "}
                      {formatMoney(
                        item.monthlyContribution,
                        profile.baseCurrency,
                      )}
                    </small>
                  </strong>
                  <em>
                    {projection
                      ? `模拟达成 ${projection.estimatedSuccessPct.toFixed(0)}%`
                      : item.targetDate}
                  </em>
                  <span className="row-actions">
                    <button
                      aria-label="编辑目标"
                      onClick={() => editGoal(item)}
                    >
                      <Edit3 size={14} />
                    </button>
                    <button
                      aria-label="删除目标"
                      onClick={() => removeGoal(item)}
                    >
                      <Trash2 size={14} />
                    </button>
                  </span>
                </div>
              );
            })}
          </div>
        )}
        <div className="form-grid compact-grid">
          <label>
            <span>目标名称</span>
            <input
              value={goal.name}
              onChange={(e) => setGoal({ ...goal, name: e.target.value })}
              placeholder="例如：长期养老账户"
            />
          </label>
          <NumberField
            label="目标金额"
            value={goal.targetAmount}
            onChange={(v) => setGoal({ ...goal, targetAmount: Number(v) })}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="已经投入"
            value={goal.currentAmount}
            onChange={(v) => setGoal({ ...goal, currentAmount: Number(v) })}
            prefix={profile.baseCurrency}
          />
          <NumberField
            label="计划每月投入"
            value={goal.monthlyContribution}
            onChange={(v) =>
              setGoal({ ...goal, monthlyContribution: Number(v) })
            }
            prefix={profile.baseCurrency}
          />
          <label>
            <span>目标日期</span>
            <input
              type="date"
              value={goal.targetDate}
              onChange={(e) => setGoal({ ...goal, targetDate: e.target.value })}
            />
          </label>
          <label>
            <span>目标优先级</span>
            <select
              value={goal.priority}
              onChange={(e) =>
                setGoal({
                  ...goal,
                  priority: e.target.value as Goal["priority"],
                })
              }
            >
              <option>刚性</option>
              <option>重要</option>
              <option>弹性</option>
            </select>
          </label>
        </div>
        <div className="form-actions">
          {editingGoalId ? (
            <button
              className="text-button"
              onClick={() => {
                setEditingGoalId(null);
                setGoal(emptyGoal);
              }}
            >
              取消修改
            </button>
          ) : (
            <span />
          )}
          <button
            className="secondary"
            onClick={persistGoal}
            disabled={saving || !goal.name}
          >
            {editingGoalId ? <Save size={16} /> : <Plus size={16} />}
            {editingGoalId ? "保存目标" : "添加目标"}
          </button>
        </div>
      </section>
    </div>
  );
}
