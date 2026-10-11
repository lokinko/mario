import { useRequestGuard } from "../../lib/useRequestGuard";
import { useState } from "react";
import {
  getFxSettings,
  getSnapshot,
  refreshFxSettings,
  setFxOverride,
} from "../../api";
import type { FxBook, Snapshot } from "../../types";

const currencies = ["CNY", "USD", "HKD", "EUR", "JPY", "GBP"];
export function ExchangeRates({
  baseCurrency,
  onUpdate,
}: {
  baseCurrency: string;
  onUpdate: (snapshot: Snapshot) => void;
}) {
  const request = useRequestGuard();
  const [book, setBook] = useState<FxBook | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [drafts, setDrafts] = useState<Record<string, string>>({});
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
  return (
    <details
      className="panel facts-advanced exchange-settings"
      onToggle={(e) => {
        if (e.currentTarget.open && !book && !busy) void run(getFxSettings);
      }}
    >
      <summary>汇率设置</summary>
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
          <p className="section-intro">
            基准币种：{book.baseCurrency} · 最近检查：
            {book.checkedAt
              ? new Date(book.checkedAt * 1000).toLocaleString("zh-CN")
              : "尚未更新"}
          </p>
          {book.error && (
            <p className="warning-text" role="alert">
              自动更新暂未成功，沿用上次可用值；没有可用值的币种暂停换算。
              {book.error}
            </p>
          )}
          <div className="exchange-rate-list">
            {currencies
              .filter((c) => c !== baseCurrency)
              .map((currency) => {
                const manual = book.manual[currency];
                const quote = book.quotes.find(
                  (q) => q.fromCurrency === currency,
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
                  <div className="exchange-rate-row" key={currency}>
                    <div>
                      <strong>
                        1 {currency} ={" "}
                        {value == null
                          ? "待更新"
                          : Number(value.toPrecision(10))}{" "}
                        {baseCurrency}
                      </strong>
                      <small>
                        {manual ? "手动汇率" : "自动 · ECB"} · 观察日{" "}
                        {manual?.observedOn ?? quote?.observedOn ?? "暂无"}
                        {!manual &&
                        quote &&
                        quote.observedOn !== book.requestedOn
                          ? "（沿用最近工作日）"
                          : ""}
                      </small>
                    </div>
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
                        placeholder={value == null ? "输入汇率" : String(value)}
                        onChange={(e) =>
                          setDrafts({ ...drafts, [currency]: e.target.value })
                        }
                      />
                    </label>
                    <button
                      className="secondary"
                      disabled={busy || !valid}
                      onClick={() =>
                        void run(async () => {
                          const next = await setFxOverride(
                            currency,
                            Number(draft),
                          );
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
                        onClick={() =>
                          void run(() => setFxOverride(currency, null))
                        }
                      >
                        恢复自动
                      </button>
                    )}
                  </div>
                );
              })}
          </div>
          <p className="section-intro">
            参考汇率按实际发布日标注，周末和节假日沿用最近工作日。
            <a
              href="https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html"
              target="_blank"
              rel="noreferrer"
            >
              查看来源
            </a>
          </p>
        </>
      )}
      <button
        className="secondary"
        disabled={busy}
        onClick={() => void run(refreshFxSettings)}
      >
        更新自动汇率
      </button>
    </details>
  );
}
