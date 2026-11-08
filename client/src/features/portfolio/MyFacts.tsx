import { Button, Tabs } from "antd";
import { Drawer } from "../../components/Drawer";
import { AssetAmountRow } from "./AssetAmountRow";
import { ExchangeRates } from "./ExchangeRates";
import { AssetEditor } from "./AssetEditor";
import { AssetOverview } from "./AssetOverview";
import { useAssetHistory } from "./useAssetHistory";
import { Callout } from "../../components/Callout";
import type { DisplayPreferenceProps } from "../../components/DisplayPreferences";
import { HoldingDetail } from "./HoldingDetail";
import { HoldingTrades } from "./HoldingTrades";
import { HoldingsList, defaultHoldingFilters } from "./HoldingsList";
import { InvestmentOverview } from "./InvestmentOverview";
import { ProfileForm } from "./ProfileForm";
import { sortInvestmentHoldings } from "./holdingMetrics";
import { useHoldingRemoval } from "./useHoldingRemoval";
import { useEffect, useRef, useState } from "react";
import { Plus, RefreshCw } from "lucide-react";
import { refreshSecurityPrices } from "../../api/portfolio";
import type { DataIssue, Holding, Snapshot } from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { emptyHolding } from "./forms";
import "./portfolio.css";

const defaultCashCurrencies = ["CNY", "HKD", "USD"];

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

export type FactsSection = "holdings" | "savings";

export function MyFacts({
  snapshot,
  onUpdate,
  flash,
  section: requestedSection,
  onSectionChange,
  display = { maskAmounts: false, setMaskAmounts: () => {} },
}: {
  display?: Pick<DisplayPreferenceProps, "maskAmounts" | "setMaskAmounts">;
  section?: FactsSection;
  onSectionChange?: (section: FactsSection) => void;
  snapshot: Snapshot;
  onUpdate: (snapshot: Snapshot) => void;
  flash: (message: string) => void;
}) {
  const history = useAssetHistory(snapshot);
  const [fxCurrency, setFxCurrency] = useState<string | null>(null);
  const [issueTarget, setIssueTarget] = useState<string | null>(null);
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
  const [ownSection, setOwnSection] = useState<FactsSection>("holdings");
  const section = requestedSection ?? ownSection;
  const setSection = onSectionChange ?? setOwnSection;
  const cash = snapshot.holdings.filter((item) => item.assetClass === "现金");
  const cashCurrencies = [
    ...new Set([...defaultCashCurrencies, ...cash.map((h) => h.currency)]),
  ];
  useEffect(() => {
    if (!issueTarget) return;
    const row = document.getElementById(`asset-${issueTarget}`);
    row?.scrollIntoView?.({ block: "center", behavior: "smooth" });
    row?.focus();
    setIssueTarget(null);
  }, [issueTarget]);
  const locateIssue = (issue: DataIssue) => {
    const id = issue.holdingIds[0];
    const target = snapshot.holdings.find(
      (h) => h.id === (snapshot.optionContracts?.[id]?.underlyingId ?? id),
    );
    if (!target) return;
    setSection(target.assetClass === "现金" ? "savings" : "holdings");
    setFilters({ ...defaultHoldingFilters, showClosed: true });
    setIssueTarget(target.id);
  };
  const repairIssue = (issue: DataIssue) => {
    const holding = snapshot.holdings.find((h) =>
      issue.holdingIds.includes(h.id),
    );
    if (!holding) return;
    if (issue.code === "missingFxRate")
      setFxCurrency(issue.currency ?? holding.currency);
    else if (issue.code === "cashNeedsReconciliation") openEditor(holding);
    else setSelectedId(holding.id);
  };
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
            <Button type="link" size="small" onClick={() => openEditor(item)}>
              添加
            </Button>
          </div>
        </div>
      );
    const tradeSummary = snapshot.holdingTradeSummaries?.[item.id];
    return (
      <div
        key={item.id || item.currency}
        id={`asset-${item.id}`}
        tabIndex={-1}
        className="facts-asset-entry"
      >
        <AssetAmountRow
          issues={snapshot.valuationStatus.issues.filter((issue) =>
            issue.holdingIds.some(
              (id) =>
                id === item.id ||
                snapshot.optionContracts?.[id]?.underlyingId === item.id,
            ),
          )}
          onRepair={repairIssue}
          source={
            snapshot.holdingValuations?.find((v) => v.holdingId === item.id)
              ?.providerName
          }
          observedOn={
            snapshot.holdingValuations?.find((v) => v.holdingId === item.id)
              ?.observedOn
          }
          priceBasis={
            snapshot.holdingValuations?.find((v) => v.holdingId === item.id)
              ?.priceBasis
          }
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
  const sectionContent = (
    <>
      {!editing && !selected && errorNotice}
      {section === "holdings" &&
        !priceWarningDismissed &&
        Boolean(snapshot.priceRefreshErrors?.length) && (
          <Callout tone="info" onClose={() => setPriceWarningDismissed(true)}>
            部分价格更新失败，已保留原值：
            {snapshot.priceRefreshErrors?.join("；")}
          </Callout>
        )}
      <InvestmentOverview
        holdings={section === "holdings" ? investments : cash}
        accounting={displayAccounting}
        baseCurrency={currency}
        cash={section === "savings"}
      />
      <section
        className={`panel form-panel ${section === "holdings" ? "investment-panel" : "cash-panel"}`}
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
      {section === "savings" && (
        <ProfileForm
          profile={profile}
          savedBaseCurrency={snapshot.profile.baseCurrency}
          onChange={setProfile}
          busy={busy}
          run={run}
          onUpdate={onUpdate}
          flash={flash}
        />
      )}
    </>
  );
  return (
    <div className="page facts-page">
      <PageHeader
        title="我的情况"
        action={
          section === "holdings" && (
            <div className="trade-toolbar facts-heading-actions">
              {investments.length > 0 && (
                <Button
                  icon={<RefreshCw size={15} />}
                  disabled={busy || Boolean(editing)}
                  onClick={refreshPrices}
                >
                  {busy ? "更新中…" : "更新自动价格"}
                </Button>
              )}
              <Button
                type="primary"
                icon={<Plus size={16} />}
                disabled={busy || Boolean(editing) || addingTrade}
                onClick={() => {
                  setTradeHolding(undefined);
                  setAddingTrade(true);
                }}
              >
                记录交易
              </Button>
              <Button
                icon={<Plus size={16} />}
                disabled={busy || Boolean(editing) || addingTrade}
                onClick={add}
              >
                添加持仓
              </Button>
            </div>
          )
        }
      />
      {fxCurrency && (
        <Drawer
          title={`补充 ${fxCurrency} 汇率`}
          onClose={() => setFxCurrency(null)}
        >
          <ExchangeRates
            baseCurrency={currency}
            usedCurrencies={[fxCurrency]}
            onUpdate={onUpdate}
          />
        </Drawer>
      )}
      <AssetOverview
        snapshot={snapshot}
        onLocateIssue={locateIssue}
        issuesDisabled={
          busy || Boolean(editing) || Boolean(selected) || addingTrade
        }
        history={history}
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
      <Tabs
        aria-label="我的情况分类"
        className="facts-tabs"
        activeKey={section}
        destroyOnHidden
        onChange={(id) => {
          setSection(id as FactsSection);
          setError("");
        }}
        items={(
          [
            ["holdings", "投资持仓"],
            ["savings", "存款与收支"],
          ] as const
        ).map(([id, label]) => ({
          key: id,
          label: <span className="facts-tab-label">{label}</span>,
          disabled: busy || Boolean(editing) || addingTrade,
          children: section === id ? sectionContent : null,
        }))}
      />
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
    </div>
  );
}
