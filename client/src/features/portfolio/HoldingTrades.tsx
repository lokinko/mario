import { isPendingFinancialRequest } from "../../api/portfolio/financialRequest";
import { Callout } from "../../components/Callout";
import { FinancialHistory } from "./FinancialHistory";
import { useDrawerControls } from "../../components/Drawer";
import { useEffect, useRef, useState } from "react";
import { Plus, X } from "lucide-react";
import {
  deleteHoldingTrade,
  saveHoldingTrade,
  updateHoldingTrade,
} from "../../api/portfolio";
import type {
  Holding,
  HoldingTrade,
  HoldingTradeFields,
  HoldingTradeKind,
  Snapshot,
  OptionContract,
} from "../../types";
import { formatMoney, formatNumber, formatSigned } from "../../lib/format";
import { localDateValue } from "../../lib/dates";

const tradeNames: Record<HoldingTradeKind, string> = {
  buy: "买入",
  sell: "卖出",
  dividend: "现金分红",
  expire: "到期作废",
};
const pageSize = 5;
const decimal = /^(?:0|[1-9]\d*)(?:\.\d{1,8})?$/;
const validNumber = (s: string) => decimal.test(s) && Number(s) <= 1e15;
// Only estimate the broker's gross amount here; all accounting is performed on the server.
function estimate(
  quantity: string,
  price: string,
  currency: string,
  multiplier = "1",
) {
  if (!validNumber(quantity) || !validNumber(price)) return "";
  const digits =
    new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency,
    }).resolvedOptions().maximumFractionDigits ?? 2;
  const scaled = (s: string) => {
    const [whole, fraction = ""] = s.split(".");
    return BigInt(whole) * 100000000n + BigInt(fraction.padEnd(8, "0"));
  };
  const divisor = 10n ** BigInt(16 - digits);
  const rounded =
    (scaled(quantity) * scaled(price) * BigInt(Number(multiplier)) +
      divisor / 2n) /
    divisor;
  const s = rounded.toString().padStart(digits + 1, "0");
  return digits ? s.slice(0, -digits) + "." + s.slice(-digits) : s;
}
type Draft = Omit<HoldingTradeFields, "sequence"> & { sequence: string };
const emptyDraft = (account = ""): Draft => ({
  kind: "buy",
  occurredOn: localDateValue(new Date()),
  sequence: "1",
  quantity: "",
  price: "",
  grossAmount: "",
  fees: "0",
  taxes: "0",
  account,
  externalId: "",
  note: "",
});

export function HoldingTrades({
  snapshot,
  holding,
  newOption,
  onUpdate,
  onClose,
  onEditingChange,
  flash,
}: {
  snapshot: Snapshot;
  holding?: Holding;
  newOption?: { contract: OptionContract; name: string; currency: string };
  onUpdate: (snapshot: Snapshot) => void;
  onClose?: () => void;
  onEditingChange?: (editing: boolean) => void;
  flash: (message: string) => void;
}) {
  const option = holding
    ? snapshot.optionContracts?.[holding.id]
    : newOption?.contract;
  const names = option
    ? {
        buy: option.side === "long" ? "买入开仓" : "买入平仓",
        sell: option.side === "long" ? "卖出平仓" : "卖出开仓",
        dividend: "现金分红",
        expire: "到期作废",
      }
    : tradeNames;
  const trades = (snapshot.holdingTrades ?? []).filter(
    (t) => t.holdingId === holding?.id,
  );
  const ordered = [...trades].sort(
    (a, b) =>
      b.occurredOn.localeCompare(a.occurredOn) || b.sequence - a.sequence,
  );
  const [page, setPage] = useState(1);
  const pageCount = Math.max(1, Math.ceil(ordered.length / pageSize));
  const currentPage = Math.min(page, pageCount);
  const pageStart = (currentPage - 1) * pageSize;
  const visibleTrades = ordered.slice(pageStart, pageStart + pageSize);
  const summary = holding
    ? snapshot.holdingTradeSummaries?.[holding.id]
    : undefined;
  const [uncertain, setUncertain] = useState(false);
  const [open, setOpen] = useState(!holding);
  const [editing, setEditing] = useState<HoldingTrade | null>(null);
  const [draft, setDraft] = useState<Draft>(() => ({
    ...emptyDraft(option?.account ?? summary?.account),
    kind:
      newOption?.contract.side === "short"
        ? ("sell" as const)
        : ("buy" as const),
  }));
  const [name, setName] = useState("");
  const [symbol, setSymbol] = useState("");
  const [assetClass, setAssetClass] = useState<Holding["assetClass"]>("股票");
  const [currency, setCurrency] = useState(
    holding?.currency ??
      newOption?.currency ??
      (["CNY", "HKD", "USD"].includes(snapshot.profile.baseCurrency)
        ? snapshot.profile.baseCurrency
        : "CNY"),
  );
  const [settlementMode, setSettlementMode] = useState<"settle" | "history">(
    "settle",
  );
  const quarantined = Boolean(
    holding && snapshot.clearedTradeHoldings?.includes(holding.id),
  );
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    onEditingChange?.(open || uncertain || busy);
    return () => onEditingChange?.(false);
  }, [open, uncertain, busy, onEditingChange]);
  const [error, setError] = useState("");
  const pendingAction = useRef<(() => Promise<Snapshot>) | null>(null);
  const pendingDelete = useRef(false);
  const cashBaselines = useRef(snapshot.cashBalances ?? []);
  const drawer = useDrawerControls();
  useEffect(() => {
    drawer.setBusy(busy || uncertain);
    return () => drawer.setBusy(false);
  }, [busy, uncertain, drawer]);
  const pending = useRef(false);
  const attempt = useRef<{ key: string; id: string } | null>(null);
  const revision = useRef(
    holding ? (snapshot.holdingRevisions?.[holding.id] ?? null) : null,
  );
  const money = (value: string | number) => formatMoney(value, currency, 2);
  const isDividend = draft.kind === "dividend";
  const isExpiry = draft.kind === "expire";
  const gross = isExpiry
    ? "0"
    : draft.grossAmount ||
      estimate(
        draft.quantity,
        draft.price,
        currency,
        option?.multiplier ?? "1",
      );
  const expense = Number(draft.fees) + Number(draft.taxes);
  const cash =
    draft.kind === "buy" ? -(Number(gross) + expense) : Number(gross) - expense;
  const valid = Boolean(
    (holding || newOption || name.trim()) &&
    draft.account.trim() &&
    draft.occurredOn &&
    draft.occurredOn <= localDateValue(new Date()) &&
    /^[1-9]\d{0,6}$/.test(draft.sequence) &&
    Number(draft.sequence) <= 1e6 &&
    validNumber(gross) &&
    (isExpiry || Number(gross) > 0) &&
    validNumber(draft.fees) &&
    validNumber(draft.taxes) &&
    (isDividend ||
      (validNumber(draft.quantity) &&
        Number(draft.quantity) > 0 &&
        validNumber(draft.price) &&
        (isExpiry || Number(draft.price) > 0))) &&
    (!option ||
      (Number.isInteger(Number(draft.quantity)) &&
        draft.occurredOn <= option.expiresOn)) &&
    Number(gross) + expense <= 1e15,
  );
  const nextSequence = (date: string) =>
    String(
      Math.max(
        0,
        ...trades.filter((t) => t.occurredOn === date).map((t) => t.sequence),
      ) + 1,
    );
  const begin = (kind: HoldingTradeKind, t?: HoldingTrade) => {
    cashBaselines.current = snapshot.cashBalances ?? [];
    revision.current = holding
      ? (snapshot.holdingRevisions?.[holding.id] ?? null)
      : null;
    setEditing(t ?? null);
    setSettlementMode("settle");
    setDraft(
      t
        ? { ...t, sequence: String(t.sequence) }
        : {
            ...emptyDraft(summary?.account),
            kind,
            sequence: nextSequence(
              kind === "expire"
                ? option!.expiresOn
                : localDateValue(new Date()),
            ),
            ...(kind === "expire"
              ? {
                  occurredOn: option!.expiresOn,
                  quantity: summary?.quantity ?? "",
                  price: "0",
                  grossAmount: "0",
                }
              : {}),
          },
    );
    attempt.current = null;
    setError("");
    setOpen(true);
  };
  const close = () => {
    setOpen(false);
    setEditing(null);
    setError("");
    onClose?.();
  };
  async function save() {
    if (!valid || pending.current || uncertain) return;
    pending.current = true;
    setBusy(true);
    setError("");
    const fields: HoldingTradeFields & {
      settlementMode: "history" | "settle";
    } = {
      kind: draft.kind,
      occurredOn: draft.occurredOn,
      sequence: Number(draft.sequence),
      quantity: isDividend ? "0" : draft.quantity,
      price: isDividend || isExpiry ? "0" : draft.price,
      grossAmount: gross,
      fees: draft.fees,
      taxes: draft.taxes,
      account: draft.account.trim(),
      externalId: draft.externalId.trim(),
      note: draft.note.trim(),
      settlementMode: editing ? "history" : settlementMode,
    };
    const input = {
      expectedCashBaseline:
        cashBaselines.current.find((b) => b.currency === currency)
          ?.baselineId ?? "",
      ...fields,
      holdingId: holding?.id ?? "",
      expectedRevision: revision.current,
      newHolding: holding
        ? null
        : newOption
          ? {
              name: newOption.name,
              symbol: "",
              assetClass: "期权" as const,
              currency,
              option: newOption.contract,
            }
          : { name: name.trim(), symbol: symbol.trim(), assetClass, currency },
    };
    const key = JSON.stringify(input);
    if (attempt.current?.key !== key)
      attempt.current = { key, id: crypto.randomUUID() };
    try {
      pendingDelete.current = false;
      pendingAction.current = editing
        ? (() => {
            const target = editing.id;
            const request = {
              ...fields,
              expectedRevision: revision.current ?? "",
            };
            return () => updateHoldingTrade(target, request);
          })()
        : (() => {
            const request = { ...input, requestId: attempt.current!.id };
            return () => saveHoldingTrade(request);
          })();
      const next = await pendingAction.current();
      drawer.markClean();
      onUpdate(next);
      setPage(1);
      attempt.current = null;
      setDraft(emptyDraft(fields.account));
      close();
      flash(
        editing
          ? "交易已修改，持仓和收益已重新计算"
          : "真实交易已保存，持仓和收益已更新",
      );
    } catch (e) {
      setUncertain(isPendingFinancialRequest(e));
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  async function remove(t: HoldingTrade) {
    if (pending.current || open || uncertain) return;
    const expectedRevision = snapshot.holdingRevisions?.[t.holdingId];
    if (
      !window.confirm(
        `确认删除“${holding?.name}”在 ${t.occurredOn} 的第 ${t.sequence} 笔${names[t.kind]}？\n` +
          `账户：${t.account} · 金额：${money(t.grossAmount)}\n` +
          "删除后会重新计算持仓、成本和盈亏。当前核对基准仍计入的交易会退回买入支出或扣回卖出、分红收入；历史交易保留当前存款并标记待核对。删除最后一笔交易会将持仓数量和成本归零，保留证券信息。原始记录和本次更正会保留在更正历史中。",
      )
    )
      return;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      if (!expectedRevision) throw new Error("持仓版本缺失，请刷新后重试");
      pendingDelete.current = true;
      pendingAction.current = () =>
        deleteHoldingTrade(t.id, { expectedRevision });
      onUpdate(await pendingAction.current());
      flash("交易已删除，持仓和盈亏已更新");
    } catch (e) {
      setUncertain(isPendingFinancialRequest(e));
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }

  async function recheck() {
    if (busy || !pendingAction.current) return;
    pending.current = true;
    setBusy(true);
    try {
      const next = await pendingAction.current();
      onUpdate(next);
      setUncertain(false);
      setError("");
      if (!pendingDelete.current) {
        drawer.markClean();
        setDraft(emptyDraft(draft.account));
        attempt.current = null;
        close();
      }
      flash("操作结果已确认，未重新创建交易");
    } catch (e) {
      setUncertain(isPendingFinancialRequest(e));
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  const field = (
    key: keyof Draft,
    label: string,
    options: { type?: string; maxLength?: number; required?: boolean } = {},
  ) => (
    <label>
      <span>{label}</span>
      <input
        type={options.type ?? "text"}
        inputMode={
          ["quantity", "price", "grossAmount", "fees", "taxes"].includes(key)
            ? "decimal"
            : undefined
        }
        value={draft[key]}
        maxLength={options.maxLength ?? 32}
        required={options.required}
        max={key === "occurredOn" ? localDateValue(new Date()) : undefined}
        onChange={(e) =>
          setDraft((d) => ({
            ...d,
            [key]: e.target.value,
            ...(key === "occurredOn" && !editing
              ? { sequence: nextSequence(e.target.value) }
              : {}),
          }))
        }
      />
    </label>
  );
  return (
    <div className="holding-trades">
      {quarantined && <p>请在下方预览并修复原始记录；修复前不能新增交易。</p>}
      {holding && (
        <div className="trade-toolbar">
          <span>
            {summary
              ? `${summary.account} · 按真实成交计算持仓`
              : "从最早一笔真实买入开始录入"}
          </span>
          {(option
            ? (["buy", "sell", "expire"] as const)
            : (["buy", "sell", "dividend"] as const)
          ).map((kind) => (
            <button
              key={kind}
              className="secondary"
              disabled={
                busy ||
                uncertain ||
                open ||
                quarantined ||
                (kind === "expire" &&
                  option!.expiresOn > localDateValue(new Date()))
              }
              onClick={() => begin(kind)}
            >
              {kind === "buy" && <Plus size={14} />}{" "}
              {kind === "dividend" ? "记现金分红" : `记${names[kind]}`}
            </button>
          ))}
        </div>
      )}
      {error && (
        <Callout tone="danger">
          {error}
          {open
            ? "。输入已保留，请核对后重试；如持仓已变化，请关闭后重新打开表单。"
            : "。请核对交易记录后重试；如持仓已变化，请刷新后再操作。"}
        </Callout>
      )}
      {uncertain && (
        <p role="status">
          这笔操作的结果仍待确认，输入和请求编号已固定。
          <button type="button" disabled={busy} onClick={() => void recheck()}>
            再次核对原操作
          </button>
        </p>
      )}
      {holding && (
        <FinancialHistory
          holding={holding}
          snapshot={snapshot}
          onUpdate={onUpdate}
        />
      )}
      {open && !quarantined && (
        <form
          aria-label={editing ? "编辑真实交易" : "录入真实交易"}
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <div className="panel-title">
            <h3>
              {editing
                ? "编辑交易"
                : holding
                  ? `${holding.name} · ${names[draft.kind]}`
                  : newOption
                    ? `${newOption.name} · ${names[draft.kind]}`
                    : "记录第一笔真实买入"}
            </h3>
            <button
              type="button"
              className="text-button"
              aria-label="关闭交易录入"
              disabled={busy || uncertain}
              onClick={() => {
                if (drawer.confirmDiscard()) {
                  drawer.markClean();
                  close();
                }
              }}
            >
              <X size={16} />
            </button>
          </div>
          <fieldset className="holding-fields" disabled={busy || uncertain}>
            <div className="form-grid">
              {!holding && !newOption && (
                <>
                  <label>
                    <span>证券名称</span>
                    <input
                      required
                      maxLength={200}
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                    />
                  </label>
                  <label>
                    <span>证券代码（选填）</span>
                    <input
                      maxLength={80}
                      value={symbol}
                      onChange={(e) => setSymbol(e.target.value)}
                      placeholder="例如：2020.HK"
                    />
                  </label>
                  <label>
                    <span>资产类别</span>
                    <select
                      value={assetClass}
                      onChange={(e) =>
                        setAssetClass(e.target.value as Holding["assetClass"])
                      }
                    >
                      {["股票", "基金", "债券", "黄金", "其他"].map((c) => (
                        <option key={c}>{c}</option>
                      ))}
                    </select>
                  </label>
                  <label>
                    <span>成交币种</span>
                    <select
                      value={currency}
                      onChange={(e) => setCurrency(e.target.value)}
                    >
                      {[...new Set([currency, "CNY", "HKD", "USD"])].map(
                        (c) => (
                          <option key={c}>{c}</option>
                        ),
                      )}
                    </select>
                  </label>
                </>
              )}
              {holding && (
                <label>
                  <span>交易类型</span>
                  <select
                    value={draft.kind}
                    onChange={(e) =>
                      setDraft((d) => ({
                        ...d,
                        kind: e.target.value as HoldingTradeKind,
                        grossAmount: "",
                        ...(e.target.value === "expire" && option
                          ? {
                              occurredOn: option.expiresOn,
                              quantity: summary?.quantity ?? "",
                              price: "0",
                              grossAmount: "0",
                              sequence: nextSequence(option.expiresOn),
                            }
                          : {}),
                      }))
                    }
                  >
                    {Object.entries(names)
                      .filter(([key]) =>
                        option ? key !== "dividend" : key !== "expire",
                      )
                      .map(([key, label]) => (
                        <option key={key} value={key}>
                          {label}
                        </option>
                      ))}
                  </select>
                </label>
              )}
              {field("occurredOn", isDividend ? "分红到账日期" : "成交日期", {
                type: "date",
                required: true,
              })}
              {field("sequence", "当日顺序", { required: true })}
              {field("account", "账户 / 券商", {
                maxLength: 120,
                required: true,
              })}
              {!isDividend && (
                <>
                  {field("quantity", option ? "成交张数" : "成交数量", {
                    required: true,
                  })}
                  {!isExpiry &&
                    field(
                      "price",
                      `${option ? "每单位权利金" : "成交单价"}（${currency}）`,
                      {
                        required: true,
                      },
                    )}
                </>
              )}
              <label>
                <span>
                  {isDividend
                    ? `税前现金分红（${currency}）`
                    : `成交金额（${currency}）`}
                </span>
                <input
                  aria-label={
                    isDividend
                      ? `税前现金分红（${currency}）`
                      : `成交金额（${currency}）`
                  }
                  inputMode="decimal"
                  disabled={isExpiry}
                  value={draft.grossAmount}
                  placeholder={
                    isDividend
                      ? "按分红单填写"
                      : gross || "数量 × 单价，按成交单核对"
                  }
                  onChange={(e) =>
                    setDraft((d) => ({ ...d, grossAmount: e.target.value }))
                  }
                />
              </label>
              {field("fees", `交易费用（${currency}）`)}
              {field("taxes", `税费（${currency}）`)}
            </div>
            <details className="holding-details">
              <summary>成交编号和备注（选填）</summary>
              <div className="form-grid">
                {field("externalId", "券商成交编号", { maxLength: 200 })}
                {field("note", "交易备注", { maxLength: 2000 })}
              </div>
            </details>
            {option && (
              <p className="section-intro">
                合约乘数 {option.multiplier}；成交额 = 张数 × 每单位权利金 ×
                乘数。空头市值计为负债。到期作废仅用于已核对未行权、未被指派的合约，交割请先核对券商单据。
              </p>
            )}
            <p className="trade-amount">
              {cash < 0 ? "实际支出" : "实际净收入"}{" "}
              <strong>
                <span className="num" data-money>
                  {money(Math.abs(Number.isFinite(cash) ? cash : 0))}
                </span>
              </strong>
              <small>
                成交 / 分红{" "}
                <span className="num" data-money>
                  {money(gross)}
                </span>{" "}
                · 费用{" "}
                <span className="num" data-money>
                  {money(draft.fees)}
                </span>{" "}
                · 税费{" "}
                <span className="num" data-money>
                  {money(draft.taxes)}
                </span>
              </small>
              <small>
                {currency} 存款余额{" "}
                <span className="num" data-money>
                  {money(
                    snapshot.holdings.find(
                      (item) =>
                        item.assetClass === "现金" &&
                        item.currency === currency,
                    )?.marketValue ?? 0,
                  )}
                </span>
              </small>
            </p>
            {holding && !trades.length && (
              <p className="section-intro">
                首次录入将按真实成交重建这项持仓，替换目前的手工数量和成本。请从最早的买入开始补齐历史，不会自动补入期初余额。
              </p>
            )}
            {!editing && (
              <label>
                资金处理
                <select
                  value={settlementMode}
                  onChange={(e) =>
                    setSettlementMode(e.target.value as "history" | "settle")
                  }
                >
                  <option value="settle">新增成交，结算当前存款</option>
                  <option value="history">历史补录，当前余额已包含</option>
                </select>
              </label>
            )}
            <p className="section-intro">
              同一账户、同一证券使用同一项持仓。当日顺序按实际发生先后填写，可补录历史交易。
              买入从 {currency}{" "}
              新增结算会扣加当前存款；历史补录仅重建持仓，不重放已包含在余额中的现金流。余额核对基准之前的交易更正会提示重新核对。成交单价不改变当前行情。
              {!holding &&
                "新证券在取得行情前暂以首笔买入价估值，请及时核对当前价格。"}
              {isDividend &&
                " 现金分红计入收益，不减少持仓成本；红利再投资请另录一笔真实买入。"}
            </p>
            <div className="form-actions">
              <button
                className="primary"
                type="submit"
                disabled={!valid || uncertain}
              >
                {busy
                  ? "保存中…"
                  : editing
                    ? "保存修改并更新持仓"
                    : "保存交易并更新持仓"}
              </button>
            </div>
          </fieldset>
        </form>
      )}
      {summary && (
        <p className="trade-summary">
          持仓 {formatNumber(summary.quantity)} 单位 · 剩余成本{" "}
          <span className="num" data-money>
            {money(summary.costBasis)}
          </span>{" "}
          · 已结算盈亏{" "}
          <strong
            className={
              Number(summary.settledProfit) === 0
                ? ""
                : Number(summary.settledProfit) > 0
                  ? "gain"
                  : "loss"
            }
          >
            <span className="num" data-money>
              {formatSigned(summary.settledProfit, money)}
            </span>
          </strong>
          <small>
            卖出盈亏{" "}
            <span className="num" data-money>
              {money(summary.realizedProfit)}
            </span>{" "}
            · 净现金分红{" "}
            <span className="num" data-money>
              {money(summary.dividendIncome)}
            </span>{" "}
            · 费用{" "}
            <span className="num" data-money>
              {money(summary.totalFees)}
            </span>{" "}
            · 税费{" "}
            <span className="num" data-money>
              {money(summary.totalTaxes)}
            </span>
          </small>
        </p>
      )}
      {ordered.length > 0 && (
        <>
          <div className="table-scroll">
            <table className="trade-table">
              <thead>
                <tr>
                  <th>日期 / 顺序</th>
                  <th>类型 / 数量</th>
                  <th>成交 / 分红金额</th>
                  <th>费用 / 税费</th>
                  <th>实际现金收支</th>
                  <th>账户 / 凭据</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {visibleTrades.map((t) => (
                  <tr key={t.id}>
                    <td data-label="日期 / 顺序">
                      {t.occurredOn}
                      <small>第 {t.sequence} 笔</small>
                    </td>
                    <td data-label="类型 / 数量">
                      {t.kind === "dividend" ? (
                        names[t.kind]
                      ) : (
                        <strong className={`trade-kind-${t.kind}`}>
                          {names[t.kind]}
                        </strong>
                      )}
                      {t.kind !== "dividend" && (
                        <small>
                          {formatNumber(t.quantity, 4)} ×{" "}
                          {formatNumber(t.price, 4)}
                        </small>
                      )}
                    </td>
                    <td data-label="成交 / 分红金额">
                      <span className="num" data-money>
                        {money(t.grossAmount)}
                      </span>
                      <small>{t.currency}</small>
                    </td>
                    <td data-label="费用 / 税费">
                      <span className="num" data-money>
                        {money(t.fees)}
                      </span>
                      <small>
                        税费{" "}
                        <span className="num" data-money>
                          {money(t.taxes)}
                        </span>
                      </small>
                    </td>
                    <td data-label="实际现金收支">
                      <span className="num" data-money>
                        {money(summary?.cashFlows[t.id] ?? "0")}
                      </span>
                      {summary?.profits[t.id] !== undefined && (
                        <small>
                          卖出盈亏{" "}
                          <span className="num" data-money>
                            {money(summary.profits[t.id])}
                          </span>
                        </small>
                      )}
                    </td>
                    <td data-label="账户 / 凭据" className="trade-account">
                      {t.account}
                      <small>{t.externalId}</small>
                      <small>{t.note}</small>
                    </td>
                    <td className="trade-actions">
                      <button
                        type="button"
                        className="text-button trade-edit"
                        disabled={busy || uncertain || open}
                        onClick={() => begin(t.kind, t)}
                      >
                        编辑交易
                      </button>
                      <button
                        type="button"
                        className="text-button danger-text"
                        disabled={busy || uncertain || open}
                        onClick={() => void remove(t)}
                      >
                        删除交易
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <nav className="trade-pagination" aria-label="交易明细分页">
            <span>
              共 {ordered.length} 条 · 最新在前 · 每页 {pageSize} 条
            </span>
            <div>
              <button
                type="button"
                className="secondary"
                disabled={currentPage === 1 || busy || uncertain || open}
                onClick={() => setPage(currentPage - 1)}
              >
                上一页
              </button>
              <span aria-live="polite">
                第 {currentPage} / {pageCount} 页
              </span>
              <button
                type="button"
                className="secondary"
                disabled={
                  currentPage === pageCount || busy || uncertain || open
                }
                onClick={() => setPage(currentPage + 1)}
              >
                下一页
              </button>
            </div>
          </nav>
        </>
      )}
      {!trades.length && holding && !open && (
        <p className="empty">
          还没有真实交易记录。请按券商成交单录入，不会自动生成期初或撤销记录。
        </p>
      )}
    </div>
  );
}
