import { Button, Tabs, Tag } from "antd";
import { Drawer } from "../../components/Drawer";
import { Money } from "../../components/AmountPrivacy";
import { EmptyState } from "../../components/EmptyState";
import { AssetAmountRow } from "./AssetAmountRow";
import { ExchangeRates } from "./ExchangeRates";
import { ExchangeRateAccess } from "./ExchangeRateAccess";
import { AssetEditor } from "./AssetEditor";
import { AllocationChart } from "./AllocationChart";
import { AssetOverview } from "./AssetOverview";
import { useAssetHistory } from "./useAssetHistory";
import { Callout } from "../../components/Callout";
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
import type { DataIssue, Holding, Snapshot } from "../../types/user";
import { PageHeader } from "../../components/PageHeader";
import { formatAmount } from "../../lib/format";
import { emptyHolding } from "./forms";
import "./portfolio.css";

function sumDecimalStrings(values: string[]): string {
  let maxScale = 0;
  const parts = values.map((raw) => {
    const text = raw.trim();
    if (!/^-?\d+(?:\.\d+)?$/.test(text))
      return { neg: false, whole: "0", frac: "" };
    const neg = text.startsWith("-");
    const [whole, frac = ""] = text.replace(/^-/, "").split(".");
    if (frac.length > maxScale) maxScale = frac.length;
    return { neg, whole, frac };
  });
  const factor = 10n ** BigInt(maxScale);
  let total = 0n;
  for (const { neg, whole, frac } of parts) {
    const units =
      BigInt(whole) * factor + BigInt(frac.padEnd(maxScale, "0") || "0");
    total += neg ? -units : units;
  }
  if (total === 0n) return "0";
  const neg = total < 0n;
  const abs = (neg ? -total : total).toString().padStart(maxScale + 1, "0");
  if (maxScale === 0) return (neg ? "-" : "") + abs;
  const intPart = abs.slice(0, -maxScale);
  const fracPart = abs.slice(-maxScale).replace(/0+$/, "");
  return (neg ? "-" : "") + intPart + (fracPart ? "." + fracPart : "");
}

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
  onSaved,
}: {
  section?: FactsSection;
  onSectionChange?: (section: FactsSection) => void;
  onSaved?: () => void;
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
  const currency = snapshot.profile.baseCurrency;
  const cash = snapshot.holdings.filter((item) => item.assetClass === "现金");
  const cashAmountOf = (item: Holding) =>
    snapshot.cashBalances?.find((b) => b.holdingId === item.id)?.amount ??
    snapshot.accounting?.holdings[item.id]?.marketValue ??
    String(item.marketValue);
  const cashGroups = Array.from(new Set(cash.map((item) => item.currency)))
    .sort((a, b) =>
      a === currency ? -1 : b === currency ? 1 : a.localeCompare(b),
    )
    .map((code) => {
      const groupHoldings = cash
        .filter((item) => item.currency === code)
        .sort(
          (a, b) =>
            Number(cashAmountOf(b)) - Number(cashAmountOf(a)) ||
            a.name.localeCompare(b.name, "zh-CN"),
        );
      return {
        currency: code,
        holdings: groupHoldings,
        total: sumDecimalStrings(groupHoldings.map(cashAmountOf)),
      };
    });
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
  const openNewCashEditor = () =>
    openEditor({
      ...emptyHolding(currency),
      id: "",
      name: "",
      assetClass: "现金",
    });
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
    const tradeSummary = snapshot.holdingTradeSummaries?.[item.id];
    return (
      <div
        key={item.id}
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
          holding={withExactValues(snapshot, item)}
          exact={displayAccounting?.holdings[item.id]}
          confirmedOn={
            snapshot.cashBalances?.find((b) => b.holdingId === item.id)
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
          canDelete={true}
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
      {section === "holdings" && (
        <InvestmentOverview
          holdings={investments}
          accounting={displayAccounting}
          baseCurrency={currency}
        />
      )}
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
        ) : cash.length === 0 ? (
          <div
            className="facts-assets"
            role="table"
            aria-label="现金列表"
            aria-colcount={4}
          >
            <div role="row">
              <div role="cell" aria-colspan={4}>
                <EmptyState
                  title="还没有存款账户"
                  action={
                    <Button type="primary" onClick={openNewCashEditor}>
                      添加第一个存款账户
                    </Button>
                  }
                >
                  可按微信、支付宝、各银行卡、公积金等账户分别记录余额。
                </EmptyState>
              </div>
            </div>
          </div>
        ) : (
          <>
            <div
              className="holdings-toolbar cash-toolbar"
              data-presentation="true"
            >
              <div className="position-filters">
                <strong>存款账户</strong>
                <Tag variant="filled">{cash.length} 个</Tag>
              </div>
            </div>
            <div
              className="facts-assets"
              role="table"
              aria-label="现金列表"
              aria-colcount={4}
            >
              <div role="row" className="holdings-list-header cash-list-header">
                {["账户", "余额", "核对日期", "操作"].map((label) => (
                  <span role="columnheader" key={label}>
                    {label}
                  </span>
                ))}
              </div>
              {cashGroups.map((group) => (
                <div
                  role="rowgroup"
                  aria-label={`${group.currency} 存款账户`}
                  key={group.currency}
                >
                  <div
                    role="row"
                    className="holdings-currency-header cash-currency-header"
                  >
                    <div role="cell" aria-colspan={4}>
                      <span className="cash-currency-meta">
                        <strong>{group.currency}</strong>
                        <span>{group.holdings.length} 个账户</span>
                      </span>
                      <Money as="strong" className="num cash-currency-total">
                        {formatAmount(group.total, group.currency)}
                      </Money>
                    </div>
                  </div>
                  {group.holdings.map(renderAssetRow)}
                </div>
              ))}
            </div>
          </>
        )}
      </section>
      {section === "holdings" && <AllocationChart snapshot={snapshot} />}
      {section === "savings" && (
        <ProfileForm
          profile={profile}
          savedBaseCurrency={snapshot.profile.baseCurrency}
          onChange={setProfile}
          busy={busy}
          run={run}
          onUpdate={onUpdate}
          onSaved={onSaved}
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
          <div className="trade-toolbar facts-heading-actions">
            <ExchangeRateAccess snapshot={snapshot} onUpdate={onUpdate} />
            {section === "holdings" && investments.length > 0 && (
              <Button
                icon={<RefreshCw size={15} />}
                disabled={busy || Boolean(editing)}
                onClick={refreshPrices}
              >
                {busy ? "刷新中…" : "刷新"}
              </Button>
            )}
            {section === "holdings" && (
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
            )}
            {section === "savings" && (
              <Button
                type="primary"
                icon={<Plus size={16} />}
                disabled={busy || Boolean(editing)}
                onClick={openNewCashEditor}
              >
                添加存款账户
              </Button>
            )}
          </div>
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
                : "添加存款账户"
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
