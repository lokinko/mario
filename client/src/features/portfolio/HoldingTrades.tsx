import { useRef, useState } from "react";
import { Plus, X } from "lucide-react";
import { saveHoldingTrade } from "../../api";
import type {
  Holding,
  HoldingTrade,
  HoldingTradeInput,
  Snapshot,
} from "../../types";
import { formatMoney as formatCurrency } from "../../lib/format";
import { localDateValue } from "../../lib/dates";
import { emptyHolding } from "./forms";

const formatMoney = (value: number, currency: string) =>
  formatCurrency(value, currency, 2);

export function HoldingTrades({
  snapshot,
  holding,
  onUpdate,
  onClose,
  flash,
}: {
  snapshot: Snapshot;
  holding?: Holding;
  onUpdate: (snapshot: Snapshot) => void;
  onClose?: () => void;
  flash: (message: string) => void;
}) {
  const trades = (snapshot.holdingTrades ?? []).filter(
    (t) => t.holdingId === holding?.id,
  );
  const voided = new Set(trades.flatMap((t) => (t.voidOf ? [t.voidOf] : [])));
  const ordered = [...trades].sort(
    (a, b) =>
      a.occurredOn.localeCompare(b.occurredOn) ||
      Number(a.kind !== "opening") - Number(b.kind !== "opening") ||
      a.createdAt.localeCompare(b.createdAt) ||
      a.id.localeCompare(b.id),
  );
  let quantity = 0,
    basis = 0,
    realized = 0;
  const profits = new Map<string, number>();
  for (const t of ordered) {
    if (voided.has(t.id) || t.kind === "void") continue;
    if (t.kind === "opening" || t.kind === "buy") {
      quantity += t.quantity;
      basis += t.quantity * t.price + t.fees;
    } else {
      const soldCost = quantity > 0 ? (basis * t.quantity) / quantity : 0;
      const profit = t.quantity * t.price - t.fees - soldCost;
      profits.set(t.id, profit);
      realized += profit;
      const previousQuantity = quantity;
      quantity = Math.max(0, quantity - t.quantity);
      if (quantity < previousQuantity * 1e-12) quantity = 0;
      basis = quantity === 0 ? 0 : basis - soldCost;
    }
  }
  const [open, setOpen] = useState(!holding);
  const [kind, setKind] = useState<"buy" | "sell">("buy");
  const [name, setName] = useState("");
  const [symbol, setSymbol] = useState("");
  const [assetClass, setAssetClass] = useState<Holding["assetClass"]>("股票");
  const [currency, setCurrency] = useState(
    holding?.currency ?? snapshot.profile.baseCurrency,
  );
  const [date, setDate] = useState(localDateValue(new Date()));
  const [units, setUnits] = useState("");
  const [price, setPrice] = useState("");
  const [fees, setFees] = useState("");
  const [account, setAccount] = useState(
    [...trades].reverse().find((t) => t.account)?.account ?? "",
  );
  const [externalId, setExternalId] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const pending = useRef(false);
  const attempt = useRef<{ key: string; id: string } | null>(null);
  const referenceRevision = useRef(
    holding ? (snapshot.holdingRevisions?.[holding.id] ?? null) : null,
  );
  const gross = Number(units) * Number(price);
  const amount = kind === "buy" ? gross + Number(fees) : gross - Number(fees);
  const firstTrade = holding && !trades.length;
  const valid = Boolean(
    (holding || name.trim()) &&
    account.trim() &&
    units.trim() &&
    price.trim() &&
    Number(units) > 0 &&
    Number(price) > 0 &&
    [Number(units), Number(price), Number(fees), gross + Number(fees)].every(
      (n) => Number.isFinite(n) && n >= 0 && n <= 1e15,
    ) &&
    date &&
    date <= localDateValue(new Date()) &&
    (!holding || date >= (ordered[0]?.occurredOn ?? holding.valuationDate)) &&
    (kind !== "sell" || Number(units) <= (holding?.quantity ?? 0)) &&
    (!firstTrade ||
      (holding.quantity != null &&
        (holding.quantity === 0 || holding.costBasis > 0))),
  );
  async function submit(input: Omit<HoldingTradeInput, "requestId">) {
    if (pending.current) return;
    pending.current = true;
    setBusy(true);
    setError("");
    const key = JSON.stringify(input);
    if (attempt.current?.key !== key)
      attempt.current = { key, id: crypto.randomUUID() };
    try {
      const next = await saveHoldingTrade({
        ...input,
        requestId: attempt.current!.id,
      });
      onUpdate(next);
      referenceRevision.current =
        next.holdingRevisions?.[holding?.id ?? ""] ?? null;
      attempt.current = null;
      setUnits("");
      setPrice("");
      setFees("");
      setExternalId("");
      setNote("");
      setOpen(false);
      flash(
        input.kind === "void"
          ? "误录已撤销，持仓已重新计算"
          : "交易已保存，数量和成本已更新",
      );
      onClose?.();
    } catch (e) {
      setError(String(e));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }
  const start = (side: "buy" | "sell") => {
    referenceRevision.current = holding
      ? (snapshot.holdingRevisions?.[holding.id] ?? null)
      : null;
    setKind(side);
    setOpen(true);
    setError("");
  };
  const cancelTrade = (t: HoldingTrade) => {
    const reason = window.prompt(
      `撤销 ${t.occurredOn} 的${t.kind === "buy" ? "买入" : "卖出"} ${t.quantity} 单位。原记录保留，请填写误录原因：`,
    );
    if (!reason?.trim()) return;
    void submit({
      holdingId: holding!.id,
      expectedRevision: snapshot.holdingRevisions?.[holding!.id] ?? null,
      newHolding: null,
      kind: "void",
      occurredOn: localDateValue(new Date()),
      quantity: 0,
      price: 0,
      fees: 0,
      account: t.account,
      externalId: "",
      note: reason.trim(),
      voidOf: t.id,
    });
  };
  return (
    <div className="holding-trades">
      {holding && (
        <div className="trade-toolbar">
          <span>
            {trades.length
              ? "持仓数量与成本由交易明细计算"
              : "尚未关联交易明细"}
          </span>
          <button
            className="secondary"
            disabled={busy || open}
            onClick={() => start("buy")}
          >
            <Plus size={14} />
            记买入
          </button>
          <button
            className="secondary"
            disabled={busy || open || !holding.quantity}
            onClick={() => start("sell")}
          >
            记卖出
          </button>
        </div>
      )}
      {error && (
        <p className="error-box" role="alert">
          {error}。输入已保留；持仓有变化时请关闭表单后重新核对。
        </p>
      )}
      {open && (
        <form
          aria-label={holding ? `${holding.name}交易录入` : "新持仓交易录入"}
          onSubmit={(e) => {
            e.preventDefault();
            if (!valid) return;
            const newHolding = holding
              ? null
              : {
                  ...emptyHolding(currency),
                  name: name.trim(),
                  symbol: symbol.trim(),
                  assetClass,
                  valuationDate: date,
                  quantity: 0,
                  currentPrice: Number(price),
                };
            void submit({
              holdingId: holding?.id ?? "",
              expectedRevision: referenceRevision.current,
              newHolding,
              kind,
              occurredOn: date,
              quantity: Number(units),
              price: Number(price),
              fees: Number(fees),
              account: account.trim(),
              externalId: externalId.trim(),
              note: note.trim(),
              voidOf: null,
            });
          }}
        >
          <div className="panel-title">
            <h3>
              {holding
                ? `${holding.name} · ${kind === "buy" ? "买入" : "卖出"}`
                : "从第一笔买入开始"}
            </h3>
            <button
              type="button"
              className="text-button"
              aria-label="关闭交易录入"
              disabled={busy}
              onClick={() => {
                setOpen(false);
                onClose?.();
              }}
            >
              <X size={16} />
            </button>
          </div>
          <fieldset className="holding-fields" disabled={busy}>
            <div className="form-grid">
              {!holding && (
                <>
                  <label>
                    <span>资产名称</span>
                    <input
                      autoFocus
                      required
                      maxLength={200}
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                      placeholder="例如：某只基金 / 安踏体育"
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
                      {[
                        ...new Set([
                          currency,
                          "CNY",
                          "HKD",
                          "USD",
                          "EUR",
                          "JPY",
                          "GBP",
                        ]),
                      ].map((c) => (
                        <option key={c}>{c}</option>
                      ))}
                    </select>
                  </label>
                </>
              )}
              <label>
                <span>成交日期</span>
                <input
                  required
                  type="date"
                  min={
                    holding
                      ? (ordered[0]?.occurredOn ?? holding.valuationDate)
                      : undefined
                  }
                  max={localDateValue(new Date())}
                  value={date}
                  onChange={(e) => setDate(e.target.value)}
                />
              </label>
              <label>
                <span>账户 / 券商</span>
                <input
                  required
                  maxLength={120}
                  value={account}
                  onChange={(e) => setAccount(e.target.value)}
                  placeholder="例如：港股账户；不同账户请分别建持仓"
                />
              </label>
              <label>
                <span>成交数量</span>
                <input
                  required
                  type="number"
                  min={0}
                  max={1e15}
                  step="any"
                  inputMode="decimal"
                  value={units}
                  onChange={(e) => setUnits(e.target.value)}
                />
              </label>
              <label>
                <span>成交单价（{currency}）</span>
                <input
                  required
                  type="number"
                  min={0}
                  max={1e15}
                  step="any"
                  inputMode="decimal"
                  value={price}
                  onChange={(e) => setPrice(e.target.value)}
                />
              </label>
            </div>
            <details className="holding-details">
              <summary>费用、成交编号和备注（选填）</summary>
              <div className="form-grid">
                <label>
                  <span>手续费与税费合计（{currency}）</span>
                  <input
                    type="number"
                    min={0}
                    max={1e15}
                    step="any"
                    value={fees}
                    placeholder="0"
                    onChange={(e) => setFees(e.target.value)}
                  />
                </label>
                <label>
                  <span>券商成交编号</span>
                  <input
                    maxLength={200}
                    value={externalId}
                    onChange={(e) => setExternalId(e.target.value)}
                    placeholder="用于核对并防止重复录入"
                  />
                </label>
                <label className="full">
                  <span>交易备注</span>
                  <input
                    maxLength={2000}
                    value={note}
                    onChange={(e) => setNote(e.target.value)}
                  />
                </label>
              </div>
            </details>
            <p className="trade-amount">
              {kind === "buy" ? "买入支出" : "卖出净收入"}{" "}
              <strong>
                {formatMoney(Number.isFinite(amount) ? amount : 0, currency)}
              </strong>
              <small>
                成交额{" "}
                {formatMoney(Number.isFinite(gross) ? gross : 0, currency)} ·
                费用 {formatMoney(Number(fees) || 0, currency)}
              </small>
            </p>
            {firstTrade && (
              <p className="section-intro">
                首次记录会把 {holding.valuationDate} 的{" "}
                {holding.quantity ?? "未补充"} 单位、总成本{" "}
                {formatMoney(holding.costBasis, currency)}{" "}
                保留为期初余额。只能录入该日及之后的交易；此前明细不视为已核实。缺少数量或成本时请先编辑持仓补充。
              </p>
            )}
            <p className="section-intro">
              按券商成交记录填写。买入费用计入成本，卖出按加权平均成本计算已实现盈亏。录入后请核对当前价格；存款余额需按账户实际余额另行更新。
            </p>
            <div className="form-actions">
              <button className="primary" type="submit" disabled={!valid}>
                {busy ? "保存中…" : "保存交易并更新持仓"}
              </button>
            </div>
          </fieldset>
        </form>
      )}
      {trades.length > 0 && (
        <>
          <p className="trade-summary">
            持仓 {holding?.quantity ?? 0} 单位 · 剩余成本{" "}
            {formatMoney(holding?.costBasis ?? 0, currency)} · 已实现盈亏{" "}
            <strong className={realized >= 0 ? "gain" : "loss"}>
              {formatMoney(realized, currency)}
            </strong>
            <small>含已录入买卖费用；不含分红和期初之前的盈亏</small>
          </p>
          <div className="table-scroll">
            <table className="trade-table">
              <thead>
                <tr>
                  <th>日期 / 类型</th>
                  <th>数量 × 单价</th>
                  <th>费用 / 金额</th>
                  <th>账户 / 凭据</th>
                  <th>核对</th>
                </tr>
              </thead>
              <tbody>
                {[...ordered].reverse().map((t) => (
                  <tr
                    key={t.id}
                    className={voided.has(t.id) ? "trade-voided" : ""}
                  >
                    <td>
                      {t.occurredOn}
                      <small>
                        {
                          {
                            opening: "期初余额",
                            buy: "买入",
                            sell: "卖出",
                            void: "撤销记录",
                          }[t.kind]
                        }
                        {voided.has(t.id) ? " · 已撤销" : ""}
                      </small>
                    </td>
                    <td>
                      {t.kind === "void" ? (
                        "—"
                      ) : (
                        <>
                          {t.quantity} × {t.price}
                          <small>{t.currency}</small>
                        </>
                      )}
                    </td>
                    <td>
                      {t.kind === "void" ? (
                        "—"
                      ) : (
                        <>
                          {formatMoney(
                            t.quantity * t.price +
                              (t.kind === "sell" ? -t.fees : t.fees),
                            currency,
                          )}
                          <small>
                            费用 {formatMoney(t.fees, currency)}
                            {profits.has(t.id)
                              ? ` · 已实现 ${formatMoney(profits.get(t.id)!, currency)}`
                              : ""}
                          </small>
                        </>
                      )}
                    </td>
                    <td>
                      {t.account || "期初记录"}
                      <small>{t.externalId || ""}</small>
                      <small>{t.note}</small>
                    </td>
                    <td>
                      {["buy", "sell"].includes(t.kind) &&
                        !voided.has(t.id) && (
                          <button
                            className="text-button"
                            disabled={busy || open}
                            onClick={() => cancelTrade(t)}
                          >
                            撤销误录
                          </button>
                        )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
