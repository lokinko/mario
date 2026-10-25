import { Drawer } from "../../components/Drawer";
import { AssetAmountRow } from "./AssetAmountRow";
import { AssetEditor } from "./AssetEditor";
import { AssetOverview } from "./AssetOverview";
import { useAssetHistory } from "./useAssetHistory";
import { Callout } from "../../components/Callout";
import type { DisplayPreferenceProps } from "../../components/DisplayPreferences";
import { AssetHistory } from "./AssetHistory";
import { HoldingDetail } from "./HoldingDetail";
import { HoldingTrades } from "./HoldingTrades";
import { HoldingsList, defaultHoldingFilters } from "./HoldingsList";
import { InvestmentOverview } from "./InvestmentOverview";
import { ProfileForm } from "./ProfileForm";
import { sortInvestmentHoldings } from "./holdingMetrics";
import { useHoldingRemoval } from "./useHoldingRemoval";
import { useRef, useState } from "react";
import { Plus, RefreshCw } from "lucide-react";
import { refreshSecurityPrices } from "../../api/portfolio";
import type { Holding, Snapshot } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { formatMoney, formatNumber } from "../../lib/format";
import { emptyHolding } from "./forms";

const cashCurrencies = ["CNY", "HKD", "USD"];

function withExactValues(snapshot: Snapshot, holding: Holding): Holding {
  const source = snapshot.accounting?.holdings[holding.id];
  return source
    ? {
        ...holding,
        exactValues: {
          marketValue: source.marketValue,
          costBasis: source.costBasis,
          quantity: source.quantity,
          costPrice: source.costPrice,
          currentPrice: source.currentPrice,
        },
      }
    : holding;
}

export function MyFacts({
  snapshot,
  onUpdate,
  flash,
  display = { maskAmounts: false, setMaskAmounts: () => {} },
}: {
  display?: Pick<DisplayPreferenceProps, "maskAmounts" | "setMaskAmounts">;
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const history = useAssetHistory(snapshot);
  const [help, setHelp] = useState<"instructions" | "valuation" | null>(null);
  const [priceWarningDismissed, setPriceWarningDismissed] = useState(false);
  const [profile, setProfile] = useState(snapshot.profile);
  const [editing, setEditing] = useState<Holding | null>(null);
  const [tradeHolding, setTradeHolding] = useState<Holding>();
  const [addingTrade, setAddingTrade] = useState(false);
  const [editingTrade, setEditingTrade] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const selectedRaw = snapshot.holdings.find((item) => item.id === selectedId);
  const selected = selectedRaw
    ? withExactValues(snapshot, selectedRaw)
    : undefined;
  const [filters, setFilters] = useState(defaultHoldingFilters);
  const cashAttempt = useRef<{ key: string; id: string } | null>(null);
  const [working, setBusy] = useState(false);
  const [error, setError] = useState("");
  const removal = useHoldingRemoval({
    snapshot,
    onUpdate,
    flash,
    setWorking: setBusy,
    setError,
    onRemoved: (id) => {
      if (editing?.id === id) setEditing(null);
      if (selectedId === id) setSelectedId(null);
    },
  });
  const busy = working || removal.uncertain;
  const [section, setSection] = useState<"income" | "savings" | "holdings">(
    "holdings",
  );
  const cash = snapshot.holdings.filter((item) => item.assetClass === "现金");
  const cashRows = cashCurrencies.map(
    (currency) =>
      cash.find((item) => item.currency === currency) ?? {
        ...emptyHolding(currency),
        id: "",
        name: `${currency} 存款`,
        assetClass: "现金" as const,
      },
  );
  const investments = sortInvestmentHoldings(
    snapshot.holdings.filter(
      (item) =>
        item.assetClass !== "现金" && !snapshot.optionContracts?.[item.id],
    ),
    snapshot,
    filters.sort,
  );
  const displayAccounting = snapshot.accounting
    ? { ...snapshot.accounting, holdings: { ...snapshot.accounting.holdings } }
    : null;
  for (const [id, totals] of Object.entries(snapshot.stockOptionTotals ?? {})) {
    if (displayAccounting?.holdings[id])
      displayAccounting.holdings[id] = {
        ...displayAccounting.holdings[id],
        priceStatus: Object.entries(snapshot.optionContracts ?? {}).some(
          ([child, c]) =>
            c.underlyingId === id &&
            ["provisional", "unconfirmed"].includes(
              snapshot.accounting?.holdings[child]?.priceStatus ?? "",
            ),
        )
          ? "provisional"
          : displayAccounting.holdings[id].priceStatus,
        marketValue: totals.marketValue,
        costBasis: totals.costBasis,
        unrealizedProfit: totals.unrealizedProfit,
        settledProfit: totals.settledProfit,
        usable: totals.usable,
      };
  }
  const currency = snapshot.profile.baseCurrency;

  const run = async (
    task: () => Promise<void>,
    failure: (error: unknown) => string = String,
  ) => {
    setBusy(true);
    setError("");
    try {
      await task();
    } catch (e) {
      setError(failure(e));
    } finally {
      setBusy(false);
    }
  };
  const cashAttemptId = (key: string) => {
    if (cashAttempt.current?.key !== key)
      cashAttempt.current = { key, id: crypto.randomUUID() };
    return cashAttempt.current.id;
  };
  const openEditor = (holding: Holding) => {
    setEditing(withExactValues(snapshot, holding));
    setError("");
  };
  const refreshPrices = () =>
    run(
      async () => {
        const result = await refreshSecurityPrices();
        onUpdate(result.snapshot);
        if (result.errors.length) setError(result.errors.join("；"));
        else flash(`价格已检查，更新 ${result.updatedCount} 项持仓`);
      },
      (e) => `价格更新失败：${String(e)}`,
    );
  const add = () =>
    openEditor({
      ...emptyHolding(currency),
      id: "",
      name: "",
      assetClass: "其他",
    });
  const removeAsset = (item: Holding) =>
    void removal.remove(item, working || editingTrade);
  const errorNotice = error && (
    <Callout tone="danger">
      {error}
      {removal.uncertain && (
        <button
          type="button"
          className="secondary"
          disabled={working}
          onClick={() => void removal.reconcile()}
        >
          核对删除结果
        </button>
      )}
    </Callout>
  );
  const renderAssetRow = (item: Holding) => {
    if (item.assetClass === "现金" && !item.id)
      return (
        <div
          key={item.currency}
          role="row"
          className="asset-list-row cash-list-row unrecorded-cash"
        >
          <div role="cell">{item.currency}</div>
          <div role="cell">未记录</div>
          <div role="cell">—</div>
          <div role="cell">
            <button className="text-button" onClick={() => openEditor(item)}>
              添加
            </button>
          </div>
        </div>
      );
    const tradeSummary = snapshot.holdingTradeSummaries?.[item.id];
    return (
      <div key={item.id || item.currency} className="facts-asset-entry">
        <AssetAmountRow
          holding={withExactValues(snapshot, item)}
          exact={displayAccounting?.holdings[item.id]}
          confirmedOn={
            snapshot.cashBalances?.find((b) => b.currency === item.currency)
              ?.confirmedOn
          }
          onTrade={
            item.assetClass === "现金"
              ? undefined
              : () => {
                  setTradeHolding(withExactValues(snapshot, item));
                  setAddingTrade(true);
                }
          }
          optionTotals={snapshot.stockOptionTotals?.[item.id]}
          realizedPnl={tradeSummary?.settledProfit}
          onOpen={() => {
            if (item.assetClass === "现金") openEditor(item);
            else setSelectedId(item.id);
          }}
          disabled={busy}
          canDelete={item.assetClass !== "现金"}
          onDelete={() => removeAsset(item)}
          onEdit={() => openEditor(item)}
        />
      </div>
    );
  };
  return (
    <div className="page facts-page">
      <PageHeader
        title="我的情况"
        description="核对资产、记录成交，保留每个数字的来源与时点。"
        action={
          section === "holdings" ? (
            <div className="trade-toolbar">
              {section === "holdings" && investments.length > 0 && (
                <button
                  className="secondary"
                  disabled={busy || Boolean(editing)}
                  onClick={refreshPrices}
                >
                  <RefreshCw size={15} />
                  {busy ? "更新中…" : "更新自动价格"}
                </button>
              )}
              {section === "holdings" && (
                <button
                  className="primary"
                  disabled={busy || Boolean(editing) || addingTrade}
                  onClick={() => {
                    setTradeHolding(undefined);
                    setAddingTrade(true);
                  }}
                >
                  <Plus size={16} />
                  记录交易
                </button>
              )}
              {section === "holdings" && (
                <button
                  className="secondary"
                  disabled={busy || Boolean(editing) || addingTrade}
                  onClick={add}
                >
                  <Plus size={16} />
                  添加持仓
                </button>
              )}
            </div>
          ) : undefined
        }
      />
      <button
        className="text-button facts-help-trigger"
        onClick={() => setHelp("instructions")}
      >
        ？使用说明
      </button>
      {help && (
        <Drawer
          title={help === "valuation" ? "估值待核对原因" : "资产记录使用说明"}
          onClose={() => setHelp(null)}
        >
          {help === "valuation" ? (
            <ul>
              {snapshot.valuationStatus.warnings.map((reason) => (
                <li key={reason}>{reason}</li>
              ))}
            </ul>
          ) : (
            <>
              <h3>持仓与盈亏</h3>
              <p>
                从最早一笔真实成交开始，按账户录入买入、卖出和现金分红。数量、成本和已结算盈亏由服务端计算；首次交易替换手工数量和成本，不自动补期初余额。当前行情与成交分别管理，填写代码后可更新价格，无代码资产请手动核对。
              </p>
              <h3>存款与现金</h3>
              <p>
                按币种核对实际余额，保存后建立核对基准。新增成交可结算同币种存款；历史补录不重放已包含的现金流。核对基准之前的更正会提示重新核对余额。
              </p>
            </>
          )}
        </Drawer>
      )}
      {snapshot.valuationStatus.decisionReady === false &&
        snapshot.valuationStatus.warnings.length > 0 && (
          <Callout
            tone="warning"
            title="行动建议已暂停"
            action={
              <button
                className="text-button"
                onClick={() => setHelp("valuation")}
              >
                查看原因
              </button>
            }
          >
            {snapshot.valuationStatus.warnings.join("；")}
          </Callout>
        )}
      <AssetOverview
        snapshot={snapshot}
        history={history.history}
        display={display}
      />
      {addingTrade && (
        <Drawer title="记录交易" onClose={() => setAddingTrade(false)}>
          <HoldingTrades
            holding={tradeHolding}
            snapshot={snapshot}
            onUpdate={onUpdate}
            flash={flash}
            onClose={() => setAddingTrade(false)}
          />
        </Drawer>
      )}
      <div className="section-switcher" role="group" aria-label="我的情况分类">
        {(
          [
            ["holdings", "投资持仓"],
            ["savings", "存款与现金"],
            ["income", "收入与支出"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            aria-label={label}
            disabled={busy || Boolean(editing) || addingTrade}
            aria-pressed={section === id}
            onClick={() => {
              setSection(id);
              setError("");
            }}
          >
            {label}
            {id !== "income" && (
              <small className="num" data-money>
                {(id === "holdings"
                  ? snapshot.accounting?.investments.value.value
                  : snapshot.accounting?.cash.value.value) == null
                  ? "—"
                  : formatMoney(
                      (id === "holdings"
                        ? snapshot.accounting!.investments.value.value
                        : snapshot.accounting!.cash.value.value)!,
                      currency,
                    )}
              </small>
            )}
          </button>
        ))}
      </div>
      {!editing && !selected && errorNotice}
      {section === "holdings" &&
        !priceWarningDismissed &&
        Boolean(snapshot.priceRefreshErrors?.length) && (
          <Callout
            tone="warning"
            onClose={() => setPriceWarningDismissed(true)}
          >
            部分价格更新失败，已保留原值：
            {snapshot.priceRefreshErrors?.join("；")}
          </Callout>
        )}
      {section !== "income" && (
        <InvestmentOverview
          holdings={section === "holdings" ? investments : cash}
          accounting={displayAccounting}
          baseCurrency={currency}
          cash={section === "savings"}
        />
      )}
      {snapshot.cashBalances
        ?.filter((b) => b.needsReconciliation)
        .map((b) => (
          <Callout tone="warning" key={b.currency}>
            {b.currency}{" "}
            存款待核对：基准之前的交易发生更正，当前余额已保留。请按实际余额重新确认。
          </Callout>
        ))}
      {section === "savings" &&
        cash.some((item) => !cashCurrencies.includes(item.currency)) && (
          <Callout tone="warning">
            旧数据中还有其他币种存款，已保留并计入总资产：
            {cash
              .filter((item) => !cashCurrencies.includes(item.currency))
              .map(
                (item) => `${item.currency} ${formatNumber(item.marketValue)}`,
              )
              .join("；")}
            。请先核对并处理这些余额。
          </Callout>
        )}
      {section === "income" ? (
        <ProfileForm
          profile={profile}
          onChange={setProfile}
          busy={busy}
          run={run}
          onUpdate={onUpdate}
          flash={flash}
        />
      ) : (
        <section
          className={`panel form-panel${section === "holdings" ? " investment-panel" : ""}`}
        >
          {section === "holdings" ? (
            <HoldingsList
              snapshot={snapshot}
              holdings={investments}
              filters={filters}
              onFiltersChange={setFilters}
              renderRow={renderAssetRow}
              onRecordTrade={() => {
                setTradeHolding(undefined);
                setAddingTrade(true);
              }}
            />
          ) : (
            <div
              className="facts-assets"
              role="table"
              aria-label="现金列表"
              aria-colcount={4}
            >
              <div role="row" className="holdings-list-header cash-list-header">
                {["币种", "余额", "核对日期", "操作"].map((label) => (
                  <span role="columnheader" key={label}>
                    {label}
                  </span>
                ))}
              </div>
              <div role="rowgroup">{cashRows.map(renderAssetRow)}</div>
            </div>
          )}
        </section>
      )}

      {(editing || selected) && (
        <Drawer
          title={
            editing
              ? editing.id
                ? "编辑 " + editing.name
                : editing.assetClass === "现金"
                  ? "添加存款"
                  : "添加持仓"
              : selected!.name
          }
          size={editing ? "default" : "wide"}
          subtitle={
            selected
              ? `${selected.symbol || selected.name} · ${selected.currency} · ${selected.assetClass}`
              : undefined
          }
          resetKey={editing ? "edit" : "detail"}
          busy={busy}
          onClose={() => {
            setEditing(null);
            setSelectedId(null);
            setError("");
          }}
        >
          {errorNotice}
          {editing ? (
            <AssetEditor
              snapshot={snapshot}
              initial={editing}
              busy={busy}
              run={run}
              cashAttemptId={cashAttemptId}
              onUpdate={onUpdate}
              onSaved={(message) => {
                setEditing(null);
                flash(message);
              }}
            />
          ) : selected ? (
            <HoldingDetail
              snapshot={snapshot}
              holding={selected}
              busy={busy}
              editingTrade={editingTrade}
              onEditingTradeChange={setEditingTrade}
              onEdit={() => setEditing({ ...selected })}
              onDelete={() => removeAsset(selected)}
              onUpdate={onUpdate}
              flash={flash}
            />
          ) : null}
        </Drawer>
      )}
      <AssetHistory state={history} />
    </div>
  );
}
