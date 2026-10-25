import { Badge } from "../../components/Badge";
import { DataStamp } from "../../components/DataStamp";
import { Callout } from "../../components/Callout";
import { ExternalLink } from "../../components/ExternalLink";
import { useRequestGuard } from "../../lib/useRequestGuard";
import { useEffect, useState } from "react";
import { formatNumber } from "../../lib/format";
import {
  getFxSettings,
  getSnapshot,
  refreshFxSettings,
  setFxOverride,
} from "../../api/portfolio";
import type { FxBook, Snapshot } from "../../types";

const currencies = ["CNY", "USD", "HKD", "EUR", "JPY", "GBP"];
const visibilityKey = "mario.exchange.visible-currencies";
function readVisibility(): Record<string, boolean> {
  try {
    const saved: unknown = JSON.parse(
      localStorage.getItem(visibilityKey) ?? "{}",
    );
    if (!saved || typeof saved !== "object" || Array.isArray(saved)) return {};
    return Object.fromEntries(
      Object.entries(saved).filter(
        ([currency, visible]) =>
          /^[A-Z]{3}$/.test(currency) && typeof visible === "boolean",
      ),
    );
  } catch {
    return {};
  }
}
export function ExchangeRates({
  baseCurrency,
  usedCurrencies = [],
  onUpdate,
}: {
  baseCurrency: string;
  usedCurrencies?: string[];
  onUpdate: (snapshot: Snapshot) => void;
}) {
  const request = useRequestGuard();
  const [book, setBook] = useState<FxBook | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [visibility, setVisibility] = useState(readVisibility);
  useEffect(() => {
    const isCurrent = request.begin();
    setBusy(true);
    setBook(null);
    setDrafts({});
    setError("");
    getFxSettings()
      .then((next) => {
        if (isCurrent()) setBook(next);
      })
      .catch((e) => {
        if (isCurrent()) setError(String(e));
      })
      .finally(() => {
        if (isCurrent()) setBusy(false);
      });
    return () => request.invalidate();
  }, [baseCurrency, request]);
  const involved = new Set(usedCurrencies);
  const run = async (action: () => Promise<FxBook>) => {
    if (busy) return;
    const isCurrent = request.begin();
    setBusy(true);
    setError("");
    try {
      const next = await action();
      if (!isCurrent()) return;
      setBook(next);
      const snapshot = await getSnapshot();
      if (isCurrent()) onUpdate(snapshot);
    } catch (e) {
      if (isCurrent()) setError(String(e));
    } finally {
      if (isCurrent()) setBusy(false);
    }
  };
  const allCurrencies = [
    ...new Set([
      ...currencies,
      ...usedCurrencies,
      ...(book?.quotes
        .filter((quote) => quote.toCurrency === baseCurrency)
        .map((quote) => quote.fromCurrency) ?? []),
      ...Object.keys(book?.manual ?? {}),
    ]),
  ].filter((c) => c !== baseCurrency);
  const hasForeignCurrency = allCurrencies.some((c) => involved.has(c));
  const defaultCurrencies = allCurrencies.filter(
    (c) =>
      involved.has(c) ||
      book?.manual[c] ||
      (!hasForeignCurrency &&
        book?.quotes.some(
          (q) => q.fromCurrency === c && q.toCurrency === baseCurrency,
        )),
  );
  const selectedCurrencies = allCurrencies.filter(
    (c) => visibility[c] ?? defaultCurrencies.includes(c),
  );
  const toggleCurrency = (currency: string, visible: boolean) => {
    const next = { ...visibility, [currency]: visible };
    setVisibility(next);
    try {
      localStorage.setItem(visibilityKey, JSON.stringify(next));
    } catch {
      // Display preferences remain usable when browser storage is unavailable.
    }
  };
  const renderRate = (currency: string) => {
    if (!book) return null;
    const value =
      book.manual[currency]?.rate ??
      book.quotes.find(
        (quote) =>
          quote.fromCurrency === currency && quote.toCurrency === baseCurrency,
      )?.rate;
    return (
      <article
        className="exchange-rate-card"
        aria-label={`${currency} 兑 ${baseCurrency} 汇率`}
        key={currency}
      >
        <span className="exchange-currency-pair">
          {currency} <span aria-hidden="true">→</span> {baseCurrency}
        </span>
        <strong
          className="num"
          aria-label={`1 ${currency} = ${value == null ? "待更新" : formatNumber(value, 4, true)} ${baseCurrency}`}
        >
          {value == null ? "待更新" : formatNumber(value, 4, true)}
        </strong>
        <DataStamp
          date={
            book.manual[currency]?.observedOn ??
            book.quotes.find(
              (q) =>
                q.fromCurrency === currency && q.toCurrency === baseCurrency,
            )?.observedOn
          }
          source={book.manual[currency] ? "手动汇率" : "ECB"}
        />
        {book.manual[currency] && <Badge>手动</Badge>}
      </article>
    );
  };
  const renderEditor = (currency: string) => {
    if (!book) return null;
    const manual = book.manual[currency];
    const quote = book.quotes.find(
      (q) => q.fromCurrency === currency && q.toCurrency === baseCurrency,
    );
    const value = manual?.rate ?? quote?.rate;
    const draft = drafts[currency];
    const valid =
      draft !== undefined &&
      draft.trim() !== "" &&
      Number.isFinite(Number(draft)) &&
      Number(draft) > 0 &&
      Number(draft) <= 1e9;
    return (
      <article
        className="exchange-management-row"
        aria-label={`${currency} 汇率管理`}
        key={currency}
      >
        <div className="exchange-card-heading">
          <span className="exchange-currency-pair">
            {currency} <span aria-hidden="true">→</span> {baseCurrency}
          </span>
        </div>
        <div className="exchange-card-quote">
          <small>
            {manual ? "手动汇率" : "自动 · ECB"} · 观察日{" "}
            {manual?.observedOn ?? quote?.observedOn ?? "暂无"}
            {!manual && quote && quote.observedOn !== book.requestedOn
              ? "（沿用最近工作日）"
              : ""}
          </small>
        </div>
        <details className="exchange-rate-editor">
          <summary>修改汇率</summary>
          <label>
            <span>修改 {currency} 汇率</span>
            <input
              type="number"
              step="any"
              min="0"
              max="1000000000"
              inputMode="decimal"
              disabled={busy}
              value={draft ?? ""}
              placeholder={
                value == null ? "输入汇率" : formatNumber(value, 4, true)
              }
              onChange={(e) =>
                setDrafts({ ...drafts, [currency]: e.target.value })
              }
            />
          </label>
          <div className="exchange-card-actions">
            <button
              className="secondary"
              disabled={busy || !valid}
              onClick={() =>
                void run(async () => {
                  const next = await setFxOverride(currency, draft);
                  setDrafts((current) => {
                    const result = { ...current };
                    delete result[currency];
                    return result;
                  });
                  return next;
                })
              }
            >
              保存修改
            </button>
            {manual && (
              <button
                className="text-button"
                disabled={busy}
                onClick={() => void run(() => setFxOverride(currency, null))}
              >
                恢复自动
              </button>
            )}
          </div>
        </details>
      </article>
    );
  };
  return (
    <section className="panel exchange-settings" aria-label="汇率设置">
      <div className="panel-title exchange-settings-heading">
        <div>
          <p className="exchange-settings-meta">基准币种 {baseCurrency}</p>
        </div>
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void run(refreshFxSettings)}
        >
          更新汇率
        </button>
      </div>
      <fieldset className="exchange-currency-picker">
        <legend>展示币种</legend>
        <div>
          {allCurrencies.map((currency) => (
            <label key={currency}>
              <input
                type="checkbox"
                checked={selectedCurrencies.includes(currency)}
                onChange={(event) =>
                  toggleCurrency(currency, event.target.checked)
                }
              />
              <span>{currency}</span>
            </label>
          ))}
        </div>
      </fieldset>
      {error && <Callout tone="danger">{error}。输入已保留，可重试。</Callout>}
      {busy && <p role="status">正在更新汇率…</p>}
      {book && (
        <>
          {book.error && (
            <Callout tone="info">
              自动更新暂未成功，沿用上次可用值；没有可用值的币种暂停换算。
              {book.error}
            </Callout>
          )}
          {selectedCurrencies.length > 0 ? (
            <div className="exchange-rate-list">
              {selectedCurrencies.map(renderRate)}
            </div>
          ) : (
            <p className="empty">勾选币种后显示汇率。</p>
          )}
          <details className="exchange-management">
            <summary>管理汇率</summary>
            <p className="exchange-settings-meta">
              最近检查：
              {book.checkedAt
                ? new Date(book.checkedAt * 1000).toLocaleString("zh-CN")
                : "尚未更新"}
            </p>
            <div className="exchange-management-list">
              {selectedCurrencies.map(renderEditor)}
            </div>
            <p className="section-intro exchange-source-note">
              每日自动更新，周末沿用最近工作日。展示选择不影响资产换算。手动修改持续生效，历史资产记录保持原值。
              <ExternalLink
                href="https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html"
                target="_blank"
                rel="noreferrer"
              >
                查看来源
              </ExternalLink>
            </p>
          </details>
        </>
      )}
    </section>
  );
}
