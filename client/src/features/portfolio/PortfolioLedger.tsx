import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  CircleDollarSign,
  Cloud,
  Download,
  Eye,
  LoaderCircle,
  LockKeyhole,
  Plus,
  ShieldCheck,
  Undo2,
  Upload,
} from "lucide-react";
import {
  getFxRate,
  getPortfolioCheckins,
  getPortfolioEvents,
  previewPortfolioEventImport,
  reversePortfolioEvent,
  savePortfolioEvent,
  commitPortfolioEventImport,
} from "../../api";
import type {
  FxRateQuote,
  PortfolioCheckInRecord,
  PortfolioEventInput,
  PortfolioEventImportPreview,
  PortfolioEventRecord,
  PortfolioEventType,
  Snapshot,
} from "../../types";
import { PageHeader } from "../../components/PageHeader";
import { NumberField } from "../../components/NumberField";
import { localDateValue } from "../../lib/dates";
import {
  emptyPortfolioEvent,
  portfolioEventLabels,
  nextCalendarDate,
  downloadPortfolioEventCsvTemplate,
} from "./forms";
import { normalizedQuoteRate, fxSourceLabel } from "./valuation";
import { formatMoney } from "../../lib/format";

export function PortfolioLedger({
  snapshot,
  flash,
}: {
  snapshot: Snapshot;
  flash: (message: string) => void;
}) {
  const [events, setEvents] = useState<PortfolioEventRecord[]>([]);
  const [checkins, setCheckins] = useState<PortfolioCheckInRecord[]>([]);
  const [draft, setDraft] = useState<PortfolioEventInput>(() =>
    emptyPortfolioEvent(snapshot.profile.baseCurrency),
  );
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const [csvText, setCsvText] = useState("");
  const [csvFileName, setCsvFileName] = useState("");
  const [importPreview, setImportPreview] =
    useState<PortfolioEventImportPreview | null>(null);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState("");
  const [eventFxQuote, setEventFxQuote] = useState<FxRateQuote | null>(null);
  const [eventFxLoading, setEventFxLoading] = useState(false);
  const [eventFxError, setEventFxError] = useState("");
  const [reversingEventId, setReversingEventId] = useState<string | null>(null);
  const [reversalDate, setReversalDate] = useState(() =>
    localDateValue(new Date()),
  );
  const [reversalNote, setReversalNote] = useState("");
  const [reversalSaving, setReversalSaving] = useState(false);
  const latestCheckin = checkins[0];
  const requiresAsset = ["buy", "sell", "dividend", "interest"].includes(
    draft.eventType,
  );

  const load = async () => {
    setLoading(true);
    setError("");
    try {
      const [nextEvents, nextCheckins] = await Promise.all([
        getPortfolioEvents(),
        getPortfolioCheckins(),
      ]);
      setEvents(nextEvents);
      setCheckins(nextCheckins);
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const summary = useMemo(
    () =>
      events.reduce(
        (value, item) => {
          if (item.baseCurrency !== snapshot.profile.baseCurrency) return value;
          if (item.eventType === "deposit") value.external += item.baseAmount;
          if (item.eventType === "withdrawal")
            value.external -= item.baseAmount;
          if (item.eventType === "dividend" || item.eventType === "interest")
            value.income += item.baseAmount;
          if (item.eventType === "fee" || item.eventType === "tax")
            value.costs += item.baseAmount;
          if (item.eventType === "buy" || item.eventType === "sell")
            value.turnover += item.baseAmount;
          return value;
        },
        { external: 0, income: 0, costs: 0, turnover: 0 },
      ),
    [events, snapshot.profile.baseCurrency],
  );

  const persist = async () => {
    setSaving(true);
    setError("");
    try {
      await savePortfolioEvent(draft);
      setDraft(emptyPortfolioEvent(snapshot.profile.baseCurrency));
      setEventFxQuote(null);
      setEventFxError("");
      await load();
      flash("组合流水已冻结；后续检查点会按日期自动汇总");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setSaving(false);
    }
  };

  const lookupEventFx = async () => {
    setEventFxLoading(true);
    setEventFxError("");
    try {
      const quote = await getFxRate(
        draft.currency,
        snapshot.profile.baseCurrency,
        draft.occurredOn,
      );
      setDraft((value) => ({
        ...value,
        fxRateToBase: normalizedQuoteRate(quote.rate),
        fxRateSource: quote.providerCode,
        fxRateObservedOn: quote.observedOn,
      }));
      setEventFxQuote(quote);
    } catch (nextError) {
      setEventFxQuote(null);
      setEventFxError(String(nextError));
    } finally {
      setEventFxLoading(false);
    }
  };

  const selectCsvFile = async (file?: File) => {
    setImportPreview(null);
    setImportError("");
    if (!file) {
      setCsvText("");
      setCsvFileName("");
      return;
    }
    setCsvFileName(file.name);
    try {
      setCsvText(await file.text());
    } catch (nextError) {
      setCsvText("");
      setImportError(`无法读取文件：${String(nextError)}`);
    }
  };

  const previewCsv = async () => {
    setImporting(true);
    setImportError("");
    try {
      setImportPreview(await previewPortfolioEventImport(csvText));
    } catch (nextError) {
      setImportPreview(null);
      setImportError(String(nextError));
    } finally {
      setImporting(false);
    }
  };

  const commitCsv = async () => {
    if (!importPreview) return;
    setImporting(true);
    setImportError("");
    try {
      const result = await commitPortfolioEventImport(
        csvText,
        importPreview.previewRevision,
      );
      await load();
      setCsvText("");
      setCsvFileName("");
      setImportPreview(null);
      flash(
        `已写入 ${result.insertedCount} 笔流水，跳过 ${result.duplicateCount} 笔重复记录`,
      );
    } catch (nextError) {
      setImportError(String(nextError));
    } finally {
      setImporting(false);
    }
  };

  const beginReversal = (item: PortfolioEventRecord) => {
    setReversingEventId(item.id);
    setReversalDate(
      localDateValue(new Date()) < item.occurredOn
        ? item.occurredOn
        : localDateValue(new Date()),
    );
    setReversalNote("");
    setError("");
  };

  const submitReversal = async (item: PortfolioEventRecord) => {
    setReversalSaving(true);
    setError("");
    try {
      await reversePortfolioEvent(item.id, {
        occurredOn: reversalDate,
        note: reversalNote,
      });
      setReversingEventId(null);
      setReversalNote("");
      await load();
      flash("冲正流水已追加；原记录和修正原因均已保留");
    } catch (nextError) {
      setError(String(nextError));
    } finally {
      setReversalSaving(false);
    }
  };

  return (
    <div className="page narrow ledger-page">
      <PageHeader
        eyebrow="方法论 · 组合记录"
        title="把资金变化写成可核对的流水"
        description="先分清外部入出金、内部收益成本和交易换手，再谈组合回报。已保存记录只追加、不静默改写。"
      />

      <section className="ledger-summary">
        <article>
          <span>累计净外部现金流</span>
          <strong>
            {summary.external >= 0 ? "+" : ""}
            {formatMoney(summary.external, snapshot.profile.baseCurrency)}
          </strong>
          <small>入金减出金</small>
        </article>
        <article>
          <span>累计现金收入</span>
          <strong>
            {formatMoney(summary.income, snapshot.profile.baseCurrency)}
          </strong>
          <small>分红与利息</small>
        </article>
        <article>
          <span>累计费用税费</span>
          <strong>
            {formatMoney(summary.costs, snapshot.profile.baseCurrency)}
          </strong>
          <small>不与外部现金流混合</small>
        </article>
        <article>
          <span>累计交易额</span>
          <strong>
            {formatMoney(summary.turnover, snapshot.profile.baseCurrency)}
          </strong>
          <small>买入与卖出，仅衡量换手</small>
        </article>
      </section>

      <div className="ledger-boundary">
        <LockKeyhole size={16} />
        <div>
          <strong>
            {latestCheckin
              ? `最近冻结边界：${latestCheckin.valuationDate ?? "旧记录无估值日"}`
              : "尚未建立组合基线"}
          </strong>
          <p>
            {latestCheckin
              ? "为保护归因链，新增流水必须晚于该日期。历史漏项请在说明中保留纠正原因，并从新基线开始。"
              : "请先在决策总览冻结当前组合；基线之前的历史变化不会被系统猜测。"}
          </p>
        </div>
      </div>
      {error && (
        <div className="error-box">
          <AlertTriangle size={17} />
          {error}
        </div>
      )}

      <section className="panel form-panel ledger-entry">
        <div className="panel-title">
          <div>
            <span>新增不可变记录</span>
            <h2>这笔资金变化是什么？</h2>
          </div>
          <CircleDollarSign size={21} className="muted-icon" />
        </div>
        <div className="form-grid compact-grid">
          <label>
            <span>流水类型</span>
            <select
              value={draft.eventType}
              onChange={(event) =>
                setDraft({
                  ...draft,
                  eventType: event.target.value as PortfolioEventType,
                })
              }
            >
              {Object.entries(portfolioEventLabels).map(([value, label]) => (
                <option value={value} key={value}>
                  {label}
                </option>
              ))}
            </select>
            <small>入金/出金属于外部现金流；其他项目属于组合内部活动。</small>
          </label>
          <label>
            <span>关联资产{requiresAsset ? "" : "（可选）"}</span>
            <input
              maxLength={200}
              value={draft.assetName}
              onChange={(event) =>
                setDraft({ ...draft, assetName: event.target.value })
              }
              placeholder="例如：全球股票指数基金"
            />
          </label>
          <NumberField
            label={`金额（${draft.currency}）`}
            value={draft.amount}
            onChange={(value) => setDraft({ ...draft, amount: Number(value) })}
            prefix={draft.currency}
          />
          <label>
            <span>原币种</span>
            <select
              value={draft.currency}
              onChange={(event) => {
                setDraft({
                  ...draft,
                  currency: event.target.value,
                  fxRateToBase: null,
                  fxRateSource: "",
                  fxRateObservedOn: "",
                });
                setEventFxQuote(null);
                setEventFxError("");
              }}
            >
              {["CNY", "USD", "HKD", "EUR", "JPY", "GBP"].map((value) => (
                <option key={value}>{value}</option>
              ))}
            </select>
          </label>
          {draft.currency !== snapshot.profile.baseCurrency && (
            <>
              <NumberField
                label={`折算汇率（1 ${draft.currency} = ? ${snapshot.profile.baseCurrency}）`}
                value={draft.fxRateToBase ?? 0}
                onChange={(value) => {
                  setDraft({
                    ...draft,
                    fxRateToBase: value ? Number(value) : null,
                    fxRateSource: "",
                    fxRateObservedOn: "",
                  });
                  setEventFxQuote(null);
                }}
                suffix={snapshot.profile.baseCurrency}
              />
              <div className="fx-lookup">
                <button
                  className="secondary"
                  disabled={eventFxLoading || !draft.occurredOn}
                  onClick={lookupEventFx}
                >
                  {eventFxLoading ? (
                    <LoaderCircle className="spin" size={15} />
                  ) : (
                    <Cloud size={15} />
                  )}
                  查询 ECB 当日参考汇率
                </button>
                {eventFxError && (
                  <small className="fx-error">{eventFxError}</small>
                )}
                {draft.fxRateSource && (
                  <small>
                    已采用 {fxSourceLabel(draft.fxRateSource)} · 观察日{" "}
                    {draft.fxRateObservedOn}
                  </small>
                )}
                {eventFxQuote && (
                  <p>
                    {eventFxQuote.stalenessDays
                      ? `非工作日，使用此前 ${eventFxQuote.stalenessDays} 天的共同观察值。`
                      : "已取得当日共同观察值。"}
                    <a
                      href={eventFxQuote.sourceUrl}
                      target="_blank"
                      rel="noreferrer"
                    >
                      核对原始数据
                    </a>
                  </p>
                )}
              </div>
            </>
          )}
          <label>
            <span>发生日期</span>
            <input
              type="date"
              min={
                latestCheckin?.valuationDate
                  ? nextCalendarDate(latestCheckin.valuationDate)
                  : undefined
              }
              max={localDateValue(new Date())}
              value={draft.occurredOn}
              onChange={(event) => {
                setDraft({
                  ...draft,
                  occurredOn: event.target.value,
                  fxRateToBase: null,
                  fxRateSource: "",
                  fxRateObservedOn: "",
                });
                setEventFxQuote(null);
                setEventFxError("");
              }}
            />
            <small>日期用于自动分段和现金流权重。</small>
          </label>
          <label className="ledger-note">
            <span>核对说明</span>
            <textarea
              maxLength={2000}
              value={draft.note}
              onChange={(event) =>
                setDraft({ ...draft, note: event.target.value })
              }
              placeholder="例如：工资结余转入证券账户；以银行流水为准"
            />
          </label>
        </div>
        <div className="form-actions">
          <p>
            <ShieldCheck size={16} />
            买卖额不改变组合总值，也不会被系统当作收益。
          </p>
          <button
            className="primary"
            disabled={
              saving ||
              !latestCheckin?.valuationDate ||
              draft.occurredOn <= (latestCheckin?.valuationDate ?? "") ||
              draft.amount <= 0 ||
              !draft.occurredOn ||
              !draft.note.trim() ||
              (requiresAsset && !draft.assetName.trim()) ||
              Boolean(
                draft.currency !== snapshot.profile.baseCurrency &&
                (!draft.fxRateToBase || draft.fxRateToBase <= 0),
              )
            }
            onClick={persist}
          >
            <Plus size={16} />
            {saving ? "保存中…" : "冻结这笔流水"}
          </button>
        </div>
      </section>

      <section className="panel ledger-import">
        <div className="panel-title">
          <div>
            <span>批量录入 · 两阶段确认</span>
            <h2>从券商或银行 CSV 导入</h2>
          </div>
          <Upload size={21} className="muted-icon" />
        </div>
        <p className="import-intro">
          本机先解析并逐行校验，不会在预览时写入。相同{" "}
          <code>source + external_id</code>{" "}
          且内容一致的记录会跳过；编号相同但内容不同会阻止整批导入。
        </p>
        <div className="import-controls">
          <label className="file-picker">
            <Upload size={16} />
            <span>{csvFileName || "选择 CSV 文件"}</span>
            <input
              key={csvFileName || "empty"}
              type="file"
              accept=".csv,text/csv"
              onChange={(event) => void selectCsvFile(event.target.files?.[0])}
            />
          </label>
          <button
            className="secondary"
            onClick={() =>
              downloadPortfolioEventCsvTemplate(snapshot.profile.baseCurrency)
            }
          >
            <Download size={16} />
            下载模板
          </button>
          <button
            className="secondary"
            disabled={!csvText || importing || !latestCheckin?.valuationDate}
            onClick={previewCsv}
          >
            {importing ? (
              <LoaderCircle className="spin" size={16} />
            ) : (
              <Eye size={16} />
            )}
            校验并预览
          </button>
        </div>
        <p className="import-hint">
          必填列：source、external_id、event_type、occurred_on、amount、currency、note；可选列：fx_rate_to_base、fx_rate_source、fx_rate_observed_on、asset_name。单次最多
          1000 行、2 MB。
        </p>
        {importError && (
          <div className="error-box">
            <AlertTriangle size={17} />
            {importError}
          </div>
        )}
        {importPreview && (
          <div className="import-preview">
            <div className="import-result-bar">
              <div>
                <span className="ready-dot" />
                待写入 <strong>{importPreview.readyCount}</strong>
              </div>
              <div>
                <span className="duplicate-dot" />
                重复跳过 <strong>{importPreview.duplicateCount}</strong>
              </div>
              <div>
                <span className="error-dot" />
                错误 <strong>{importPreview.errorCount}</strong>
              </div>
              <small>
                基准币种 {importPreview.baseCurrency} · 冻结至{" "}
                {importPreview.frozenThrough}
              </small>
            </div>
            <div className="import-table-wrap">
              <table className="import-table">
                <thead>
                  <tr>
                    <th>行</th>
                    <th>状态</th>
                    <th>来源 / 交易编号</th>
                    <th>类型与日期</th>
                    <th>金额</th>
                    <th>资产 / 说明</th>
                    <th>校验结果</th>
                  </tr>
                </thead>
                <tbody>
                  {importPreview.rows.map((row) => (
                    <tr
                      key={`${row.rowNumber}-${row.externalId}`}
                      className={`import-${row.status}`}
                    >
                      <td>{row.rowNumber}</td>
                      <td>
                        <span>
                          {row.status === "ready"
                            ? "可导入"
                            : row.status === "duplicate"
                              ? "重复"
                              : "错误"}
                        </span>
                      </td>
                      <td>
                        <strong>{row.source || "—"}</strong>
                        <small>{row.externalId || "—"}</small>
                      </td>
                      <td>
                        <strong>
                          {portfolioEventLabels[
                            row.eventType as PortfolioEventType
                          ] ??
                            (row.eventType || "—")}
                        </strong>
                        <small>{row.occurredOn || "—"}</small>
                      </td>
                      <td>
                        {row.amount === null
                          ? "—"
                          : formatMoney(
                              row.amount,
                              row.currency || importPreview.baseCurrency,
                            )}
                        {row.fxRateToBase && (
                          <small>汇率 {row.fxRateToBase}</small>
                        )}
                        {row.fxRateSource && (
                          <small>
                            {fxSourceLabel(row.fxRateSource)} ·{" "}
                            {row.fxRateObservedOn}
                          </small>
                        )}
                      </td>
                      <td>
                        <strong>{row.assetName || "组合账户"}</strong>
                        <small>{row.note || "—"}</small>
                      </td>
                      <td>{row.message}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="import-actions">
              <p>
                <ShieldCheck size={16} />
                有任意错误时整批不会写入；确认时会再次校验预览版本。
              </p>
              <button
                className="primary"
                disabled={
                  importing ||
                  importPreview.errorCount > 0 ||
                  importPreview.readyCount === 0
                }
                onClick={commitCsv}
              >
                <Check size={16} />
                {importing
                  ? "写入中…"
                  : `确认写入 ${importPreview.readyCount} 笔`}
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="panel ledger-history">
        <div className="panel-title">
          <div>
            <span>本地流水账</span>
            <h2>按发生日期倒序</h2>
          </div>
          <span className="history-count">{events.length} 笔</span>
        </div>
        {loading && <div className="empty">正在读取本地流水…</div>}
        {!loading && events.length === 0 && (
          <div className="empty">
            还没有流水。建立组合基线后，从下一笔真实资金变化开始记录。
          </div>
        )}
        <div className="ledger-list">
          {events.map((item) => {
            const canReverse =
              !item.reversalOfEventId &&
              !item.reversedByEventId &&
              item.amount > 0 &&
              Boolean(latestCheckin?.valuationDate) &&
              item.occurredOn > (latestCheckin?.valuationDate ?? "");
            return (
              <article
                key={item.id}
                className={`${item.reversalOfEventId ? "reversal-entry" : ""} ${item.reversedByEventId ? "reversed-entry" : ""}`}
              >
                <span className={`event-kind kind-${item.eventType}`}>
                  {item.reversalOfEventId
                    ? "冲正"
                    : portfolioEventLabels[item.eventType]}
                </span>
                <div>
                  <strong>
                    {item.assetName || "组合账户"}
                    {item.reversedByEventId && (
                      <em className="event-status">已冲正</em>
                    )}
                  </strong>
                  <p>{item.note}</p>
                  <small>
                    {item.occurredOn} · 录入{" "}
                    {new Date(item.createdAt).toLocaleString("zh-CN")}
                    {item.externalId && !item.reversalOfEventId
                      ? ` · ${item.source}/${item.externalId}`
                      : ""}
                    {item.reversalOfEventId
                      ? ` · 原记录 ${item.reversalOfEventId.slice(0, 8)}`
                      : ""}
                  </small>
                </div>
                <div className="ledger-amount">
                  <strong>{formatMoney(item.amount, item.currency)}</strong>
                  <small>
                    {item.currency === item.baseCurrency
                      ? item.baseCurrency
                      : `汇率 ${item.fxRateToBase} · ${formatMoney(item.baseAmount, item.baseCurrency)}`}
                  </small>
                  {item.fxRateSource && (
                    <small>
                      {fxSourceLabel(item.fxRateSource)} ·{" "}
                      {item.fxRateObservedOn}
                    </small>
                  )}
                </div>
                {canReverse && (
                  <button
                    className="reversal-button"
                    onClick={() =>
                      reversingEventId === item.id
                        ? setReversingEventId(null)
                        : beginReversal(item)
                    }
                  >
                    <Undo2 size={13} />
                    冲正
                  </button>
                )}
                {reversingEventId === item.id && (
                  <div className="reversal-editor">
                    <div>
                      <strong>追加冲正记录</strong>
                      <small>
                        系统将复制原流水口径并写入等额负数，原记录不会改变。
                      </small>
                    </div>
                    <label>
                      <span>冲正日期</span>
                      <input
                        type="date"
                        min={item.occurredOn}
                        max={localDateValue(new Date())}
                        value={reversalDate}
                        onChange={(event) =>
                          setReversalDate(event.target.value)
                        }
                      />
                    </label>
                    <label>
                      <span>修正原因</span>
                      <textarea
                        maxLength={2000}
                        value={reversalNote}
                        onChange={(event) =>
                          setReversalNote(event.target.value)
                        }
                        placeholder="例如：重复录入；已与券商对账单核对"
                      />
                    </label>
                    <div className="reversal-actions">
                      <button
                        className="text-button"
                        onClick={() => setReversingEventId(null)}
                      >
                        取消
                      </button>
                      <button
                        className="primary"
                        disabled={
                          reversalSaving ||
                          !reversalDate ||
                          !reversalNote.trim()
                        }
                        onClick={() => void submitReversal(item)}
                      >
                        {reversalSaving ? (
                          <LoaderCircle className="spin" size={14} />
                        ) : (
                          <Undo2 size={14} />
                        )}
                        确认追加
                      </button>
                    </div>
                  </div>
                )}
              </article>
            );
          })}
        </div>
        <p className="effectiveness-disclaimer">
          这些记录来自用户输入，系统校验结构和口径但不核验银行、券商或市场事实。当前基线之后的误录可追加冲正；已冻结周期不能回写，应建立纠正后的新基线并保留说明。
        </p>
      </section>
    </div>
  );
}
