import { ExternalLink } from "../../components/ExternalLink";
import { useId, useRef, useState } from "react";
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
  saveGoal,
  saveHolding,
  saveProfile,
  updateHolding,
  updateGoal,
} from "../../api/portfolio";
import type {
  FinancialProfile,
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
  HoldingFields,
  hasHoldingDetails,
  validHoldingDetails,
} from "./HoldingFields";
import { fxSourceLabel, holdingValueInBase } from "./valuation";
import { formatMoney, formatNumber } from "../../lib/format";

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
  const [holdingAmount, setHoldingAmount] = useState("");
  const [editingHoldingId, setEditingHoldingId] = useState<string | null>(null);
  const [holdingDetailsOpen, setHoldingDetailsOpen] = useState(false);
  const holdingNameRef = useRef<HTMLInputElement>(null);
  const marketValueHelpId = useId();
  const [goal, setGoal] = useState<Omit<Goal, "id">>(emptyGoal);
  const [editingGoalId, setEditingGoalId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [valuationQuantity, setValuationQuantity] = useState(0);
  const [valuationLoading, setValuationLoading] = useState(false);
  const [valuationError, setValuationError] = useState("");
  const holdingBusy = saving || valuationLoading;
  const manualHoldingAmount =
    holding.assetClass === "现金" ||
    (Boolean(editingHoldingId) && !hasHoldingDetails(holding));
  const calculatedMarketValue =
    holding.quantity != null && holding.currentPrice != null
      ? formatNumber(holding.quantity * holding.currentPrice)
      : "";
  const canSaveHolding =
    !holdingBusy &&
    Boolean(holding.name.trim()) &&
    Number.isFinite(holding.marketValue) &&
    (manualHoldingAmount
      ? Boolean(holdingAmount.trim())
      : hasHoldingDetails(holding)) &&
    validHoldingDetails(holding) &&
    Number.isFinite(holding.costBasis) &&
    holding.costBasis >= 0 &&
    holding.costBasis <= 1e15 &&
    holding.marketValue >= 0 &&
    holding.marketValue <= 1e15 &&
    Boolean(holding.valuationDate) &&
    holding.valuationDate <= localDateValue(new Date());

  const updateNumber = (key: keyof FinancialProfile, value: string) =>
    setProfile({ ...profile, [key]: Number(value) });

  const persistProfile = async () => {
    setError("");
    setSaving(true);
    try {
      onUpdate(await saveProfile(profile));
      flash("财务档案已保存");
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
      setHoldingAmount("");
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
    setHoldingAmount(String(values.marketValue));
    setHoldingDetailsOpen(true);
    holdingNameRef.current?.focus();
    holdingNameRef.current?.scrollIntoView?.({
      block: "center",
      behavior: "smooth",
    });
    setEditingHoldingId(id);

    setValuationQuantity(
      values.quantity ??
        snapshot.holdingValuations.find((value) => value.holdingId === id)
          ?.quantity ??
        0,
    );
    setValuationError("");
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
        setHoldingAmount(String(values.marketValue));
      }

      flash("已冻结数量、日收盘价和来源；外币汇率由统一设置自动提供");
    } catch (nextError) {
      setValuationError(String(nextError));
    } finally {
      setValuationLoading(false);
    }
  };

  const removeHolding = async (item: Holding) => {
    setError("");
    if (!window.confirm(`确认删除“${item.name}”？`)) return;
    setSaving(true);
    try {
      onUpdate(await deleteHolding(item.id));
      if (editingHoldingId === item.id) {
        setEditingHoldingId(null);
        setHolding(emptyHolding(profile.baseCurrency));
        setHoldingAmount("");
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
    <div className="page narrow foundation-page">
      <PageHeader title="持仓与目标" />
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
        <p className="holding-hint">
          当前市值随持仓数 ×
          当前价格自动计算，无需手动输入。有证券代码时可留空价格，保存后自动获取日收盘价。现金和缺少持仓明细的旧记录可填写总金额。
        </p>
        {Boolean(snapshot.priceRefreshErrors?.length) && (
          <p className="warning-text" role="status">
            部分价格更新失败，已保留原值：
            {snapshot.priceRefreshErrors?.join("；")}
          </p>
        )}
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void persistHolding();
          }}
        >
          <fieldset
            className="holding-fields"
            disabled={saving || valuationLoading}
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
              {holding.assetClass !== "现金" && (
                <label>
                  <span>证券代码（自动价格，选填）</span>
                  <input
                    value={holding.symbol}
                    onChange={(e) =>
                      setHolding({ ...holding, symbol: e.target.value })
                    }
                    placeholder="例如：601988 / 0700.HK / AAPL"
                  />
                </label>
              )}
              {holding.assetClass !== "现金" && (
                <HoldingFields
                  holding={holding}
                  onChange={(next) => {
                    setHolding(next);
                    setHoldingAmount(String(next.marketValue));
                  }}
                />
              )}
              <label>
                <span>{`${holding.assetClass === "现金" ? "当前金额" : "当前市值"}（${holding.currency}）`}</span>
                <div className="input-affix">
                  <i>{holding.currency}</i>
                  <input
                    type="number"
                    inputMode="decimal"
                    step="any"
                    aria-label={`${holding.assetClass === "现金" ? "当前金额" : "当前市值"}（${holding.currency}） ${holding.currency}`}
                    aria-describedby={
                      manualHoldingAmount ? undefined : marketValueHelpId
                    }
                    readOnly={!manualHoldingAmount}
                    placeholder={
                      manualHoldingAmount
                        ? "手动输入"
                        : "根据持仓数和当前价格自动计算"
                    }
                    value={
                      manualHoldingAmount
                        ? holdingAmount
                        : calculatedMarketValue
                    }
                    onChange={(e) => {
                      setHoldingAmount(e.target.value);
                      setHolding({
                        ...holding,
                        marketValue: Number(e.target.value),
                      });
                    }}
                  />
                </div>
                {!manualHoldingAmount && (
                  <small id={marketValueHelpId}>
                    {holding.quantity == null
                      ? "填写持仓数后自动计算"
                      : holding.currentPrice == null
                        ? holding.symbol.trim()
                          ? "保存后获取价格并计算市值"
                          : "填写当前价格后自动计算"
                        : "当前市值 = 持仓数 × 当前价格"}
                  </small>
                )}
              </label>
              <label>
                <span>资产类别</span>
                <select
                  value={holding.assetClass}
                  onChange={(e) => {
                    if (
                      e.target.value === "现金" &&
                      hasHoldingDetails(holding)
                    ) {
                      setHoldingAmount(String(holding.marketValue));
                    }
                    setHolding({
                      ...holding,
                      assetClass: e.target.value as Holding["assetClass"],
                      ...(e.target.value === "现金"
                        ? {
                            costPrice: null,
                            quantity: null,
                            currentPrice: null,
                          }
                        : {}),
                    });
                  }}
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
                  }}
                />
                <small>日常记录可分批更新；估值日期用于核对价格口径</small>
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
                {holding.costPrice == null ? (
                  <NumberField
                    label={`累计成本（${holding.currency}）`}
                    value={holding.costBasis}
                    onChange={(v) =>
                      setHolding({ ...holding, costBasis: Number(v) })
                    }
                    prefix={holding.currency}
                  />
                ) : (
                  <p>
                    总成本：{formatMoney(holding.costBasis, holding.currency)}
                  </p>
                )}
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
                              {formatNumber(valuation.quantity)} ×{" "}
                              {formatNumber(valuation.unitPrice)}{" "}
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
                              ·{" "}
                              {valuation.priceBasis ===
                              "split_adjusted_daily_close"
                                ? "日收盘价（拆股调整）"
                                : "未复权日收盘价"}
                            </span>
                            <ExternalLink
                              href={valuation.sourceUrl}
                              target="_blank"
                              rel="noreferrer"
                            >
                              核对请求来源
                            </ExternalLink>
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
            {holding.currency !== profile.baseCurrency && (
              <p className="section-intro">
                汇率由系统自动更新，统一在“我的情况 → 汇率设置”修改。
              </p>
            )}
            <div className="form-actions">
              {editingHoldingId ? (
                <button
                  type="button"
                  className="text-button"
                  onClick={() => {
                    setHoldingDetailsOpen(false);
                    setEditingHoldingId(null);
                    setHolding(emptyHolding(profile.baseCurrency));
                    setHoldingAmount("");

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
                    {item.assetClass !== "现金" && (
                      <>
                        <small>
                          成本价：
                          {item.costPrice == null
                            ? "待补充"
                            : `${formatNumber(item.costPrice)} ${item.currency}`}
                        </small>
                        <small>
                          持仓数：
                          {item.quantity == null
                            ? "待补充"
                            : formatNumber(item.quantity)}
                        </small>
                        <small>
                          当前价格：
                          {item.currentPrice == null
                            ? "待补充"
                            : `${formatNumber(item.currentPrice)} ${item.currency}`}
                        </small>
                      </>
                    )}
                    <small>
                      {item.currency}
                      {item.currency !== profile.baseCurrency &&
                      item.fxRateToBase
                        ? ` · 汇率 ${formatNumber(item.fxRateToBase)}`
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
                      disabled={saving || valuationLoading}
                      aria-label="编辑资产"
                      onClick={() => editHolding(item)}
                    >
                      <Edit3 size={14} />
                    </button>
                    <button
                      type="button"
                      disabled={saving || valuationLoading}
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
