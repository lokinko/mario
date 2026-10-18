import { Drawer } from "../../components/Drawer";
import { AssetAmountRow } from "./AssetAmountRow";
import { AssetEditor } from "./AssetEditor";
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
import { formatNumber } from "../../lib/format";
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
}: {
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const [profile, setProfile] = useState(snapshot.profile);
  const [editing, setEditing] = useState<Holding | null>(null);
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
    <div className="error-box" role="alert">
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
    </div>
  );
  const renderAssetRow = (item: Holding) => {
    const tradeSummary = snapshot.holdingTradeSummaries?.[item.id];
    return (
      <div key={item.id || item.currency} className="facts-asset-entry">
        <AssetAmountRow
          holding={withExactValues(snapshot, item)}
          exact={displayAccounting?.holdings[item.id]}
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
    <div className="page narrow facts-page">
      <PageHeader
        title="我的情况"
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
                  onClick={() => setAddingTrade(true)}
                >
                  <Plus size={16} />
                  记录买入
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
      {addingTrade && (
        <Drawer title="记录买入" onClose={() => setAddingTrade(false)}>
          <HoldingTrades
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
            disabled={busy || Boolean(editing) || addingTrade}
            aria-pressed={section === id}
            onClick={() => {
              setSection(id);
              setError("");
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {!editing && !selected && errorNotice}
      {section === "holdings" &&
        Boolean(snapshot.priceRefreshErrors?.length) && (
          <p className="warning-text" role="status">
            部分价格更新失败，已保留原值：
            {snapshot.priceRefreshErrors?.join("；")}
          </p>
        )}
      {snapshot.valuationStatus.decisionReady === false &&
        snapshot.valuationStatus.warnings.length > 0 && (
          <p className="warning-text">
            行动建议已暂停：{snapshot.valuationStatus.warnings.join("；")}
          </p>
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
          <p className="error-box" role="alert" key={b.currency}>
            {b.currency}{" "}
            存款待核对：基准之前的交易发生更正，当前余额已保留。请按实际余额重新确认。
          </p>
        ))}
      {section === "savings" && (
        <p className="section-intro">
          按币种核对实际余额，保存后建立新的核对基准。新增成交选择结算当前存款，历史补录不重放已包含的现金流；基准之前的交易更正会提示重新核对余额。
        </p>
      )}
      {section === "savings" &&
        cash.some((item) => !cashCurrencies.includes(item.currency)) && (
          <p className="warning-text">
            旧数据中还有其他币种存款，已保留并计入总资产：
            {cash
              .filter((item) => !cashCurrencies.includes(item.currency))
              .map(
                (item) => `${item.currency} ${formatNumber(item.marketValue)}`,
              )
              .join("；")}
            。请先核对并处理这些余额。
          </p>
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
            />
          ) : (
            <div
              className="facts-assets"
              role="table"
              aria-label="现金列表"
              aria-colcount={3}
            >
              <div role="row" className="holdings-list-header cash-list-header">
                {["币种", "余额", "操作"].map((label) => (
                  <span role="columnheader" key={label}>
                    {label}
                  </span>
                ))}
              </div>
              <div role="rowgroup">{cashRows.map(renderAssetRow)}</div>
            </div>
          )}
          {section === "holdings" && (
            <details className="investment-help">
              <summary>持仓与盈亏说明</summary>
              <p className="section-intro">
                用“记录买入”从最早一笔真实成交开始，按账户分别录入买入、卖出和现金分红，数量、成本和已结算盈亏自动计算。首次交易会按成交记录替换手工数量和成本，不自动补期初余额。成交与当前行情分别管理；填写证券代码后可刷新当前价格，无代码的资产请手动核对价格。浮动盈亏反映剩余持仓，已结算盈亏包含卖出盈亏与净现金分红。新增结算扣加同币种存款；历史补录不重放已包含的现金流，核对基准之前的更正会提示重新核对余额。
              </p>
            </details>
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
      <AssetHistory snapshot={snapshot} />
    </div>
  );
}
