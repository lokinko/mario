import { Badge } from "../../components/Badge";
import { DataStamp } from "../../components/DataStamp";
import { Money } from "../../components/AmountPrivacy";
import {
  formatAmount,
  formatMoney,
  formatNumber,
  formatSigned,
} from "../../lib/format";
import type { AnalysisPreview } from "../../types/ai";

interface HoldingRow {
  id: string;
  name: string;
  symbol: string;
  currency: string;
  currentQuantity?: string;
  quantityUnit?: string;
  contract?: string;
  costBasis?: string;
  marketValue?: string;
  unrealizedProfit?: string;
  weightPct?: string;
}

interface EvidenceItem {
  id: string;
  title?: string;
  assetName?: string;
  asOfDate?: string;
  sourceTier?: string;
  publisher?: string;
  claim?: string;
}

interface MemoryItem {
  kind?: string;
  origin?: string;
  statement?: string;
  date?: string;
}

const memoryKindLabel: Record<string, string> = {
  preference: "偏好",
  viewpoint: "观点",
  context: "背景",
};

function asObject(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function asText(value: unknown): string | undefined {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed || undefined;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return String(value);
  }
  return undefined;
}

function parseHoldings(raw: unknown): HoldingRow[] {
  const map = asObject(raw);
  if (!map) return [];
  const rows: HoldingRow[] = [];
  for (const [id, entry] of Object.entries(map)) {
    const obj = asObject(entry);
    if (!obj) continue;
    rows.push({
      id,
      name: asText(obj.name) ?? id,
      symbol: asText(obj.symbol) ?? "",
      currency: asText(obj.currency) ?? "CNY",
      currentQuantity: asText(obj.currentQuantity),
      quantityUnit: asText(obj.quantityUnit),
      contract: asText(obj.contract),
      costBasis: asText(obj.costBasis),
      marketValue: asText(obj.marketValue),
      unrealizedProfit: asText(obj.unrealizedProfit),
      weightPct: asText(obj.weightPct),
    });
  }
  rows.sort((a, b) => {
    const wa = a.weightPct != null ? Number(a.weightPct) : -Infinity;
    const wb = b.weightPct != null ? Number(b.weightPct) : -Infinity;
    if (wa !== wb) return wb - wa;
    const ma = a.marketValue != null ? Number(a.marketValue) : -Infinity;
    const mb = b.marketValue != null ? Number(b.marketValue) : -Infinity;
    if (ma !== mb) return mb - ma;
    return a.name.localeCompare(b.name, "zh-CN");
  });
  return rows;
}

function parseSummary(raw: unknown): {
  baseCurrency: string;
  items: { label: string; value: string; money?: boolean }[];
} {
  const obj = asObject(raw);
  if (!obj) return { baseCurrency: "CNY", items: [] };
  const baseCurrency = asText(obj.baseCurrency) ?? "CNY";
  const items: { label: string; value: string; money?: boolean }[] = [];
  const pushMoney = (key: string, label: string, suffix = "") => {
    const value = asText(obj[key]);
    if (value != null) {
      items.push({
        label,
        value: `${formatMoney(value, baseCurrency)}${suffix}`,
        money: true,
      });
    }
  };
  pushMoney("portfolioValue", "总资产");
  pushMoney("investmentValue", "投资资产");
  pushMoney("cashValue", "存款与现金");
  pushMoney("monthlyIncome", "月收入");
  pushMoney("monthlyExpense", "月支出");
  pushMoney("monthlySurplus", "月结余");
  const emergencyMonths = asText(obj.emergencyMonths);
  pushMoney(
    "emergencyFund",
    "备用金",
    emergencyMonths ? `（${formatNumber(emergencyMonths, 2)} 个月）` : "",
  );
  pushMoney("liabilities", "负债");
  const horizon = asText(obj.horizonYears);
  if (horizon != null) {
    items.push({ label: "投资期限", value: `${horizon} 年` });
  }
  const drawdown = asText(obj.maxDrawdownPct);
  if (drawdown != null) {
    items.push({
      label: "可承受最大回撤",
      value: `${formatNumber(drawdown, 2)}%`,
    });
  }
  return { baseCurrency, items };
}

function parseEvidence(raw: unknown): EvidenceItem[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .map((entry, index): EvidenceItem | null => {
      const obj = asObject(entry);
      if (!obj) return null;
      return {
        id: asText(obj.id) ?? `e-${index}`,
        title: asText(obj.title),
        assetName: asText(obj.assetName),
        asOfDate: asText(obj.asOfDate),
        sourceTier: asText(obj.sourceTier),
        publisher: asText(obj.publisher),
        claim: asText(obj.claim),
      };
    })
    .filter((item): item is EvidenceItem => item !== null);
}

function parseMemory(raw: unknown): MemoryItem[] {
  const obj = asObject(raw);
  const notes = obj?.notes;
  if (!Array.isArray(notes)) return [];
  return notes
    .map((entry): MemoryItem | null => {
      const item = asObject(entry);
      if (!item) return null;
      const statement = asText(item.statement);
      if (!statement) return null;
      return {
        kind: asText(item.kind),
        origin: asText(item.origin),
        statement,
        date: asText(item.date) ?? asText(item.updatedAt)?.slice(0, 10),
      };
    })
    .filter((item): item is MemoryItem => item !== null);
}

export function ContextPreview({
  preview,
  previousQuestionCount,
}: {
  preview: AnalysisPreview;
  previousQuestionCount: number;
}) {
  const payload = asObject(preview.payload) ?? {};
  const question = asText(payload.question);
  const previousTurn = asObject(payload.previousTurn);
  const previousQuestion = asText(previousTurn?.question);
  const earlierQuestions = Array.isArray(previousTurn?.earlierQuestions)
    ? previousTurn.earlierQuestions
        .map(asText)
        .filter((item): item is string => Boolean(item))
    : [];
  const facts = asObject(payload.facts);
  const summary = parseSummary(facts?.summary);
  const holdings = parseHoldings(facts?.holdings);
  const evidence = parseEvidence(payload.researchEvidence);
  const memory = parseMemory(payload.memory);
  const methodology = asText(payload.investmentMethodology);

  const hasStructured =
    Boolean(question) ||
    summary.items.length > 0 ||
    holdings.length > 0 ||
    evidence.length > 0 ||
    memory.length > 0 ||
    Boolean(methodology);

  const showQuantity = holdings.some((row) => row.currentQuantity != null);
  const showCost = holdings.some((row) => row.costBasis != null);
  const showMarket = holdings.some((row) => row.marketValue != null);
  const showPnl = holdings.some((row) => row.unrealizedProfit != null);
  const showWeight = holdings.some((row) => row.weightPct != null);

  const scopeBadges = preview.groups.filter(
    (group) =>
      group.key !== "question" &&
      group.key !== "investmentMethodology" &&
      (group.included || group.key === "profile" || group.key === "holdings"),
  );

  return (
    <div className="context-preview-view">
      <div className="context-preview-meta">
        <DataStamp date={preview.generatedAt} source={preview.model} />
        <p className="muted">
          {previousQuestionCount
            ? `承接 ${previousQuestionCount} 条历史问题与上轮摘要`
            : "新对话"}
        </p>
        {scopeBadges.length > 0 && (
          <div className="context-preview-tags" aria-label="本次包含的数据范围">
            {scopeBadges.map((group) => (
              <Badge key={group.key} tone={group.included ? "info" : "neutral"}>
                {group.included
                  ? group.key === "profile"
                    ? group.label
                    : `${group.label} ${group.recordCount}`
                  : `未选 ${group.label}`}
              </Badge>
            ))}
          </div>
        )}
      </div>

      {question && (
        <section className="preview-section">
          <h3>本轮问题</h3>
          <p className="preview-prose">{question}</p>
        </section>
      )}

      {(previousQuestion || earlierQuestions.length > 0) && (
        <section className="preview-section">
          <h3>承接的对话</h3>
          <ul className="preview-list">
            {earlierQuestions.map((item, index) => (
              <li key={`earlier-${index}`}>{item}</li>
            ))}
            {previousQuestion && <li>{previousQuestion}</li>}
          </ul>
        </section>
      )}

      {summary.items.length > 0 && (
        <section className="preview-section">
          <h3>财务档案</h3>
          <dl className="preview-summary-grid">
            {summary.items.map((item) => (
              <div key={item.label}>
                <dt>{item.label}</dt>
                <dd>
                  {item.money ? (
                    <Money className="num">{item.value}</Money>
                  ) : (
                    <span className="num">{item.value}</span>
                  )}
                </dd>
              </div>
            ))}
          </dl>
        </section>
      )}

      {holdings.length > 0 && (
        <section className="preview-section">
          <h3>当前持仓（{holdings.length}）</h3>
          <div className="table-scroll">
            <table className="preview-holdings-table">
              <thead>
                <tr>
                  <th>标的</th>
                  {showQuantity && <th className="num">数量</th>}
                  {showCost && <th className="num">成本</th>}
                  {showMarket && <th className="num">市值</th>}
                  {showPnl && <th className="num">浮动盈亏</th>}
                  {showWeight && <th className="num">仓位</th>}
                </tr>
              </thead>
              <tbody>
                {holdings.map((row) => {
                  const pnlNumber =
                    row.unrealizedProfit != null
                      ? Number(row.unrealizedProfit)
                      : 0;
                  const pnlTone =
                    pnlNumber > 0 ? "gain" : pnlNumber < 0 ? "loss" : "";
                  const subline = [
                    row.symbol &&
                    !row.name.includes(row.symbol) &&
                    !row.symbol.startsWith("CASH-")
                      ? row.symbol
                      : "",
                    row.currency,
                    row.contract ?? "",
                  ]
                    .filter(Boolean)
                    .join(" · ");
                  return (
                    <tr key={row.id}>
                      <td>
                        <strong>{row.name}</strong>
                        {subline && <small>{subline}</small>}
                      </td>
                      {showQuantity && (
                        <td className="num">
                          {row.currentQuantity != null
                            ? `${formatNumber(row.currentQuantity, 4)}${row.quantityUnit ? ` ${row.quantityUnit}` : ""}`
                            : "—"}
                        </td>
                      )}
                      {showCost && (
                        <td className="num">
                          {row.costBasis != null ? (
                            <Money>
                              {formatAmount(row.costBasis, row.currency)}
                            </Money>
                          ) : (
                            "—"
                          )}
                        </td>
                      )}
                      {showMarket && (
                        <td className="num">
                          {row.marketValue != null ? (
                            <Money>
                              {formatAmount(row.marketValue, row.currency)}
                            </Money>
                          ) : (
                            "—"
                          )}
                        </td>
                      )}
                      {showPnl && (
                        <td className={`num ${pnlTone}`}>
                          {row.unrealizedProfit != null ? (
                            <Money>
                              {formatSigned(row.unrealizedProfit, (v) =>
                                formatAmount(v, row.currency),
                              )}
                            </Money>
                          ) : (
                            "—"
                          )}
                        </td>
                      )}
                      {showWeight && (
                        <td className="num">
                          {row.weightPct != null
                            ? `${formatNumber(row.weightPct, 2)}%`
                            : "—"}
                        </td>
                      )}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {evidence.length > 0 && (
        <section className="preview-section">
          <h3>研究资料（{evidence.length}）</h3>
          <ul className="preview-evidence-list">
            {evidence.map((item) => {
              const meta = [item.assetName, item.asOfDate, item.publisher]
                .filter(Boolean)
                .join(" · ");
              return (
                <li key={item.id}>
                  <strong>{item.title || "参考资料"}</strong>
                  {meta && <small>{meta}</small>}
                  {item.claim && <p className="preview-prose">{item.claim}</p>}
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {memory.length > 0 && (
        <section className="preview-section">
          <h3>长期记忆（{memory.length}）</h3>
          <ul className="preview-list">
            {memory.map((item, index) => (
              <li key={index}>
                {item.kind && (
                  <Badge>{memoryKindLabel[item.kind] ?? item.kind}</Badge>
                )}{" "}
                {item.statement}
              </li>
            ))}
          </ul>
        </section>
      )}

      {methodology && (
        <details className="preview-collapsible">
          <summary>投资方法论</summary>
          <p className="preview-prose">{methodology}</p>
        </details>
      )}

      {!hasStructured && (
        <section className="preview-section">
          <h3>发送内容</h3>
          <pre className="context-preview">
            {preview.renderedContext?.trim() ||
              JSON.stringify(preview.payload, null, 2)}
          </pre>
        </section>
      )}
    </div>
  );
}
