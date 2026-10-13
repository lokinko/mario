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
  const allCurrencies = [...new Set([...currencies, ...usedCurrencies])].filter(
    (c) => c !== baseCurrency,
  );
  const hasForeignCurrency = allCurrencies.some((c) => involved.has(c));
  const primaryCurrencies = allCurrencies.filter(
    (c) =>
      involved.has(c) ||
      book?.manual[c] ||
      (!hasForeignCurrency &&
        book?.quotes.some(
          (q) => q.fromCurrency === c && q.toCurrency === baseCurrency,
        )),
  );
  const otherCurrencies = allCurrencies.filter(
    (c) => !primaryCurrencies.includes(c),
  );
  const renderRate = (currency: string) => {
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
        className={`exchange-rate-row${involved.has(currency) ? " is-involved" : ""}`}
        aria-label={`${currency} 兑 ${baseCurrency} 汇率`}
        key={currency}
      >
        <div className="exchange-card-heading">
          <span className="exchange-currency-pair">
            {currency} <span aria-hidden="true">→</span> {baseCurrency}
          </span>
          {involved.has(currency) && (
            <span className="exchange-involved-badge">当前涉及</span>
          )}
        </div>
        <div className="exchange-card-quote">
          <strong>
            1 {currency} = {value == null ? "待更新" : formatNumber(value)}{" "}
            {baseCurrency}
          </strong>
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
              placeholder={value == null ? "输入汇率" : formatNumber(value)}
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
                  const next = await setFxOverride(currency, Number(draft));
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
          <h2>汇率设置</h2>
          <p className="exchange-settings-meta">
            基准币种 {baseCurrency} · 最近检查：
            {book?.checkedAt
              ? new Date(book.checkedAt * 1000).toLocaleString("zh-CN")
              : "尚未更新"}
          </p>
        </div>
        <button
          className="secondary"
          disabled={busy}
          onClick={() => void run(refreshFxSettings)}
        >
          更新自动汇率
        </button>
      </div>
      <p className="section-intro">
        汇率每天自动更新，所有外币资产统一使用这里的值。手动修改会保持生效，直到恢复自动；已保存的历史流水不变。
      </p>
      {error && (
        <p className="error-box" role="alert">
          {error}。输入已保留，可重试。
        </p>
      )}
      {busy && <p role="status">正在更新汇率…</p>}
      {book && (
        <>
          {book.error && (
            <p className="warning-text" role="alert">
              自动更新暂未成功，沿用上次可用值；没有可用值的币种暂停换算。
              {book.error}
            </p>
          )}
          {primaryCurrencies.length > 0 && (
            <div className="exchange-rate-list">
              {primaryCurrencies.map(renderRate)}
            </div>
          )}
          {otherCurrencies.length > 0 && (
            <details className="exchange-other-currencies">
              <summary>其他币种汇率与设置</summary>
              <div className="exchange-rate-list">
                {otherCurrencies.map(renderRate)}
              </div>
            </details>
          )}
          <p className="section-intro exchange-source-note">
            参考汇率按实际发布日标注，周末和节假日沿用最近工作日。
            <ExternalLink
              href="https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html"
              target="_blank"
              rel="noreferrer"
            >
              查看来源
            </ExternalLink>
          </p>
        </>
      )}
    </section>
  );
}
