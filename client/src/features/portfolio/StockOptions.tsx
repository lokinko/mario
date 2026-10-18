import { useEffect, useRef, useState } from "react";
import type { Holding, OptionContract, Snapshot } from "../../types";
import { HoldingTrades } from "./HoldingTrades";
import { updateOptionPrice } from "../../api/portfolio";
import { formatMoney } from "../../lib/format";
import { localDateValue } from "../../lib/dates";
import { useDrawerControls } from "../../components/Drawer";
import { useHoldingRemoval } from "./useHoldingRemoval";

export function StockOptions({
  snapshot,
  stock,
  disabled,
  onEditingChange,
  onUpdate,
  flash,
}: {
  snapshot: Snapshot;
  stock: Holding;
  disabled: boolean;
  onEditingChange: (editing: boolean) => void;
  onUpdate: (s: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const [adding, setAdding] = useState(false);
  const [prepared, setPrepared] = useState<{
    contract: OptionContract;
    name: string;
    currency: string;
  } | null>(null);
  const [selected, setSelected] = useState("");
  const [tradeEditing, setTradeEditing] = useState(false);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState("");
  const [contract, setContract] = useState<OptionContract>({
    underlyingId: stock.id,
    account: snapshot.holdingTradeSummaries?.[stock.id]?.account ?? "",
    optionType: "call",
    side: "long",
    strike: "",
    expiresOn: "",
    multiplier: "100",
  });
  const [quote, setQuote] = useState<{
    id: string;
    requestId: string;
    expectedRevision: string;
    price: string;
    observedOn: string;
  } | null>(null);
  const [uncertain, setUncertain] = useState(false);
  const pending = useRef(false);
  const drawer = useDrawerControls();
  const removal = useHoldingRemoval({
    snapshot,
    onUpdate,
    flash,
    setWorking,
    setError,
    onRemoved: () => setSelected(""),
  });
  const locked =
    disabled || working || tradeEditing || uncertain || removal.uncertain;
  useEffect(() => {
    onEditingChange(
      adding || tradeEditing || working || quote != null || removal.uncertain,
    );
    return () => onEditingChange(false);
  }, [
    adding,
    tradeEditing,
    working,
    quote,
    removal.uncertain,
    onEditingChange,
  ]);
  useEffect(() => {
    drawer.setBusy(working || uncertain || removal.uncertain);
    return () => drawer.setBusy(false);
  }, [working, uncertain, removal.uncertain, drawer]);
  const options = Object.entries(snapshot.optionContracts ?? {}).filter(
    ([, c]) => c.underlyingId === stock.id,
  );
  const totals = snapshot.stockOptionTotals?.[stock.id];
  const active = snapshot.holdings.find((h) => h.id === selected);
  const money = (value: string | number | null | undefined) =>
    value == null ? "待补" : formatMoney(value, stock.currency);
  async function saveQuote() {
    if (!quote || pending.current) return;
    pending.current = true;
    setWorking(true);
    setError("");
    try {
      const { id, ...input } = quote;
      onUpdate(await updateOptionPrice(id, input));
      setQuote(null);
      setUncertain(false);
      drawer.markClean();
      flash("期权权利金与盈亏已更新");
    } catch (e) {
      setUncertain(String(e).includes("结果待确认"));
      setError(String(e));
    } finally {
      pending.current = false;
      setWorking(false);
    }
  }
  return (
    <section aria-label="股票关联期权">
      <h3>关联期权</h3>
      <p className="section-intro">
        认购（Call）和认沽（Put）按合约分别记录，多头买入开仓、卖出平仓，空头卖出开仓、买入平仓。盈亏归入本股票，股数不受期权买卖影响。
      </p>
      {totals && (
        <dl className="detail-grid">
          <div>
            <dt>股票及期权净市值</dt>
            <dd>{totals.usable ? money(totals.marketValue) : "待核对"}</dd>
          </div>
          <div>
            <dt>合计浮动盈亏</dt>
            <dd>{totals.usable ? money(totals.unrealizedProfit) : "待核对"}</dd>
          </div>
          <div>
            <dt>合计已结算盈亏</dt>
            <dd>{totals.usable ? money(totals.settledProfit) : "待核对"}</dd>
          </div>
          <div>
            <dt>其中期权已结算盈亏</dt>
            <dd>
              {totals.usable ? money(totals.optionSettledProfit) : "待核对"}
            </dd>
          </div>
        </dl>
      )}
      {error && (
        <p role="alert" className="error-box">
          {error}
        </p>
      )}
      {removal.uncertain && (
        <button disabled={working} onClick={() => void removal.reconcile()}>
          核对期权删除结果
        </button>
      )}
      {!adding && !quote && (
        <button
          type="button"
          className="secondary"
          disabled={locked}
          onClick={() => {
            setAdding(true);
            setSelected("");
          }}
        >
          添加期权合约
        </button>
      )}
      {adding && !prepared && (
        <form
          aria-label="添加期权合约"
          onSubmit={(e) => {
            e.preventDefault();
            setPrepared({
              contract: { ...contract, account: contract.account.trim() },
              currency: stock.currency,
              name: `${Array.from(stock.name).slice(0, 100).join("")} ${contract.expiresOn} ${contract.strike} ${contract.optionType === "call" ? "认购" : "认沽"} ${contract.side === "long" ? "多头" : "空头"}`,
            });
          }}
        >
          <fieldset className="holding-fields" disabled={disabled}>
            <div className="form-grid">
              <label>
                期权账户 / 券商
                <input
                  required
                  maxLength={120}
                  value={contract.account}
                  onChange={(e) =>
                    setContract({ ...contract, account: e.target.value })
                  }
                />
              </label>
              <label>
                期权类型
                <select
                  value={contract.optionType}
                  onChange={(e) =>
                    setContract({
                      ...contract,
                      optionType: e.target
                        .value as OptionContract["optionType"],
                    })
                  }
                >
                  <option value="call">认购 Call</option>
                  <option value="put">认沽 Put</option>
                </select>
              </label>
              <label>
                持仓方向
                <select
                  value={contract.side}
                  onChange={(e) =>
                    setContract({
                      ...contract,
                      side: e.target.value as OptionContract["side"],
                    })
                  }
                >
                  <option value="long">多头（买方）</option>
                  <option value="short">空头（卖方）</option>
                </select>
              </label>
              <label>
                行权价（{stock.currency}）
                <input
                  required
                  inputMode="decimal"
                  pattern="[0-9]+([.][0-9]{1,8})?"
                  value={contract.strike}
                  onChange={(e) =>
                    setContract({ ...contract, strike: e.target.value })
                  }
                />
              </label>
              <label>
                到期日
                <input
                  required
                  type="date"
                  value={contract.expiresOn}
                  onChange={(e) =>
                    setContract({ ...contract, expiresOn: e.target.value })
                  }
                />
              </label>
              <label>
                合约乘数
                <input
                  required
                  type="number"
                  min="1"
                  max="1000000"
                  step="1"
                  value={contract.multiplier}
                  onChange={(e) =>
                    setContract({ ...contract, multiplier: e.target.value })
                  }
                />
              </label>
            </div>
            <p>
              币种沿用标的股票 {stock.currency}
              。乘数请以券商合约为准，每张通常代表多单位标的。
            </p>
            <button type="submit" className="primary">
              下一步：记录开仓
            </button>{" "}
            <button
              type="button"
              onClick={() => {
                setAdding(false);
                drawer.markClean();
              }}
            >
              取消添加期权
            </button>
          </fieldset>
        </form>
      )}
      {prepared && (
        <HoldingTrades
          key={JSON.stringify(prepared)}
          snapshot={snapshot}
          newOption={prepared}
          onEditingChange={setTradeEditing}
          onUpdate={onUpdate}
          flash={flash}
          onClose={() => {
            setPrepared(null);
            setAdding(false);
          }}
        />
      )}
      {options.length > 0 && (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>合约</th>
                <th>未平仓张数</th>
                <th>净市值</th>
                <th>浮动盈亏</th>
                <th>已结算盈亏</th>
                <th>操作</th>
              </tr>
            </thead>
            <tbody>
              {options.map(([id, c]) => {
                const h = snapshot.holdings.find((item) => item.id === id)!;
                const exact = snapshot.accounting?.holdings[id];
                return (
                  <tr key={id}>
                    <td>
                      {c.expiresOn} · {c.strike}{" "}
                      {c.optionType === "call" ? "认购" : "认沽"} ·{" "}
                      {c.side === "long" ? "多头" : "空头"}
                      <small>
                        {c.account} · 乘数 {c.multiplier}
                      </small>
                    </td>
                    <td>
                      {snapshot.holdingTradeSummaries?.[id]?.quantity ?? "0"}
                    </td>
                    <td>
                      {exact?.usable ? money(exact.marketValue) : "待核对"}
                    </td>
                    <td>
                      {exact?.usable ? money(exact.unrealizedProfit) : "待核对"}
                    </td>
                    <td>
                      {exact?.usable ? money(exact.settledProfit) : "待核对"}
                    </td>
                    <td>
                      <button
                        type="button"
                        disabled={locked || adding || !!quote}
                        onClick={() => setSelected(selected === id ? "" : id)}
                      >
                        期权交易明细
                      </button>
                      <button
                        type="button"
                        disabled={locked || adding || !!quote}
                        onClick={() => {
                          setQuote({
                            id,
                            requestId: crypto.randomUUID(),
                            expectedRevision:
                              snapshot.holdingRevisions?.[id] ?? "",
                            price: exact?.currentPrice ?? "",
                            observedOn: localDateValue(new Date()),
                          });
                        }}
                      >
                        更新期权报价
                      </button>
                      <button
                        type="button"
                        disabled={locked || adding || !!quote}
                        onClick={() => void removal.remove(h, locked)}
                      >
                        删除期权合约
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {quote && (
        <form
          aria-label="更新期权报价"
          onSubmit={(e) => {
            e.preventDefault();
            void saveQuote();
          }}
        >
          <fieldset disabled={working || uncertain} className="holding-fields">
            <div className="form-grid">
              <label>
                当前每单位权利金（{stock.currency}）
                <input
                  required
                  inputMode="decimal"
                  pattern="[0-9]+([.][0-9]{1,8})?"
                  value={quote.price}
                  onChange={(e) =>
                    setQuote({ ...quote, price: e.target.value })
                  }
                />
              </label>
              <label>
                报价日期
                <input
                  required
                  type="date"
                  max={localDateValue(new Date())}
                  value={quote.observedOn}
                  onChange={(e) =>
                    setQuote({ ...quote, observedOn: e.target.value })
                  }
                />
              </label>
            </div>
            <button type="submit">保存期权报价</button>{" "}
            <button
              type="button"
              onClick={() => {
                setQuote(null);
                drawer.markClean();
              }}
            >
              取消报价
            </button>
          </fieldset>
          {uncertain && (
            <button
              type="button"
              disabled={working}
              onClick={() => void saveQuote()}
            >
              核对原报价保存结果
            </button>
          )}
        </form>
      )}
      {active && !adding && (
        <fieldset
          disabled={disabled || working || !!quote}
          className="holding-trades-controls"
        >
          <h4>{active.name}</h4>
          <HoldingTrades
            key={active.id}
            snapshot={snapshot}
            holding={active}
            onEditingChange={setTradeEditing}
            onUpdate={onUpdate}
            flash={flash}
          />
        </fieldset>
      )}
    </section>
  );
}
