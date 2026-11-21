import { ExternalLink } from "../../components/ExternalLink";
import { useEffect, useRef, useState } from "react";
import { saveAttachmentDownload } from "../../lib/download";
import { getResearchAttachment } from "../../api/evidence";
import type {
  AnalysisEvidenceReference,
  AnalysisWorkflowTrace,
} from "../../types/ai";

const labels: Record<string, string> = {
  facts: "本次事实卡",
  question: "本轮原始消息",
  previousTurn: "历史用户原话",
  currentQuantity: "当前数量",
  quantityUnit: "数量单位",
  unrealizedProfit: "浮动盈亏",
  contract: "期权合约",
  referencePrice: "参考价",
  priceBasis: "参考价口径",
  monthlySurplus: "月结余",
  portfolioValue: "资产总额",
  investmentValue: "投资资产",
  cashValue: "存款合计",
  weightPct: "占比",
  targetPct: "目标占比",
  deviationPct: "偏差",
  profitPct: "浮盈比例",
  liabilities: "负债",
  priceDate: "价格日期",
  priceSource: "价格来源",
  financialProfile: "我的收支",
  portfolio: "我的持仓",
  currentUserMessage: "本轮原始消息",
  dailyAssetHistory: "每日资产记录",
  deterministicPlan: "规划结果",
  riskFindings: "风险检查",
  personalInvestmentRules: "投资规则",
  periodicSystemReviews: "原始复盘",
  marketValue: "市值",
  costBasis: "成本",
  monthlyIncome: "月收入",
  monthlyExpense: "月支出",
  emergencyFund: "备用金",
  emergencyMonths: "备用金月数",
  debtBalance: "负债",
  concentrationPct: "集中度",
  totalValue: "资产总额",
  content: "原始文字",
  amount: "金额",
  note: "备注",
  valuationDate: "估值日期",
  currency: "币种",
  originalPrice: "原始成交价",
  originalQuantity: "原始成交数量",
  occurredOn: "交易日期",
  grossAmount: "成交金额",
  fees: "费用",
  taxes: "税费",
  account: "交易账户",
  sequence: "同日顺序",
  kind: "交易类型",
  recordedTradeCount: "已录入交易笔数",
  confirmedOn: "余额核对日期",
  needsReconciliation: "余额待核对状态",
  horizonYears: "投资期限",
  maxDrawdownPct: "最大回撤",
};

const metadataOnlyFields = new Set([
  "name",
  "symbol",
  "currency",
  "quantityUnit",
  "baseCurrency",
]);

const percentFields = new Set([
  "weightPct",
  "targetPct",
  "deviationPct",
  "profitPct",
  "concentrationPct",
  "maxDrawdownPct",
]);

const moneyFields = new Set([
  "marketValue",
  "costBasis",
  "unrealizedProfit",
  "monthlyIncome",
  "monthlyExpense",
  "monthlySurplus",
  "emergencyFund",
  "liabilities",
  "debtBalance",
  "portfolioValue",
  "investmentValue",
  "cashValue",
  "totalValue",
  "amount",
  "grossAmount",
  "fees",
  "taxes",
  "originalPrice",
  "referencePrice",
]);

export interface PersonalFactSummary {
  id: string;
  title: string;
  detail: string;
}

function pointer(context: unknown, path: string): unknown {
  if (!path.startsWith("/")) return undefined;
  return path
    .slice(1)
    .split("/")
    .reduce<unknown>((value, key) => {
      const decoded = key.replace(/~1/g, "/").replace(/~0/g, "~");
      return value && typeof value === "object"
        ? (value as Record<string, unknown>)[decoded]
        : undefined;
    }, context);
}

function formatPersonalValue(
  field: string,
  rawValue: string,
  record?: Record<string, unknown>,
  fallbackCurrency?: string,
): string {
  if (percentFields.has(field)) return `${rawValue}%`;
  if (field === "currentQuantity") {
    const unit =
      record && typeof record.quantityUnit === "string"
        ? record.quantityUnit
        : "";
    return unit ? `${rawValue} ${unit}` : rawValue;
  }
  if (field === "horizonYears") return `${rawValue} 年`;
  if (field === "emergencyMonths") return `${rawValue} 个月`;
  if (moneyFields.has(field)) {
    const currency =
      record && typeof record.currency === "string"
        ? record.currency
        : fallbackCurrency || "";
    return currency ? `${rawValue} ${currency}` : rawValue;
  }
  return rawValue;
}

// External/saved sources and prior-turn user statements appear as clickable citations.
// Individual fields from the user's own holdings or financial profile are grouped into
// concise inline summaries instead of flooding the report with single-number footnotes.
export function reportSources(trace?: AnalysisWorkflowTrace) {
  const catalog = new Map(
    (trace?.evidenceCatalog ?? []).map((source) => [source.id, source]),
  );
  const used = new Map<string, AnalysisEvidenceReference>();
  const localByFact = new Map<number, string[]>();
  const localByInference = new Map<number, string[]>();
  const personalGroups = new Map<
    string,
    { title: string; entries: Map<string, string> }
  >();

  const summaryObj = pointer(trace?.personalContext, "/facts/summary");
  const baseCurrency =
    summaryObj &&
    typeof summaryObj === "object" &&
    "baseCurrency" in summaryObj &&
    typeof (summaryObj as Record<string, unknown>).baseCurrency === "string"
      ? String((summaryObj as Record<string, unknown>).baseCurrency)
      : "";

  const recordPersonal = (
    groupId: string,
    title: string,
    fieldLabel: string,
    formattedValue: string,
  ) => {
    const existing = personalGroups.get(groupId) ?? {
      title,
      entries: new Map<string, string>(),
    };
    if (!existing.entries.has(fieldLabel)) {
      existing.entries.set(fieldLabel, formattedValue);
    }
    personalGroups.set(groupId, existing);
  };

  const processLocal = (path: string, quote?: string): string | undefined => {
    const userHistory = path.startsWith("/previousTurn/");
    if (
      userHistory &&
      path !== "/previousTurn/question" &&
      !/^\/previousTurn\/earlierQuestions\/(0|[1-9]\d*)$/.test(path)
    )
      return undefined;
    const value = pointer(trace?.personalContext, path);
    if (
      value === undefined ||
      value === null ||
      typeof value === "object" ||
      !labels[path.split("/")[1]]
    )
      return undefined;
    // Numeric JSON representations can differ after parsing (100.0 vs 100).
    if (
      quote !== undefined &&
      (typeof value === "string"
        ? value !== quote
        : Number(quote) !== value && String(value) !== quote)
    )
      return undefined;

    const parts = path.split("/").slice(1);
    // Current-turn question is already visible right above the answer.
    if (parts[0] === "question" || parts[0] === "currentUserMessage") {
      return undefined;
    }

    if (userHistory) {
      const id = `local:${path}`;
      const title =
        parts[1] === "question"
          ? "上一轮用户原话"
          : `早期用户原话 · 摘录 ${Number(parts[2]) + 1}`;
      catalog.set(id, {
        id,
        title,
        publisher: "本次对话中的用户原话",
        sourceUrl: "",
        sourceTier: "用户声明（未改写档案）",
        notes: "用户声明（未改写档案）。",
        asOfDate: "",
        evidenceType: "personal_snapshot",
        claim: quote ?? String(value),
      });
      return id;
    }

    const field = parts.at(-1) ?? "";
    if (metadataOnlyFields.has(field)) return undefined;

    const fieldLabel = labels[field] ?? field;
    const rawValue = quote ?? String(value);
    const parent = pointer(
      trace?.personalContext,
      path.slice(0, path.lastIndexOf("/")),
    );
    const parentRecord =
      parent && typeof parent === "object"
        ? (parent as Record<string, unknown>)
        : undefined;

    // Group holding fields (including legacy /portfolio/holdings/{idx} and trade refs).
    const isFactsHolding = parts[0] === "facts" && parts[1] === "holdings";
    const isLegacyHolding = parts[0] === "portfolio" && parts[1] === "holdings";
    if ((isFactsHolding || isLegacyHolding) && parts.length >= 4) {
      const holdingPath = `/${parts.slice(0, 3).join("/")}`;
      const holding = pointer(trace?.personalContext, holdingPath);
      const holdingRecord =
        holding && typeof holding === "object"
          ? (holding as Record<string, unknown>)
          : undefined;
      const name =
        holdingRecord && typeof holdingRecord.name === "string"
          ? holdingRecord.name
          : parts[2];
      const symbol =
        holdingRecord && typeof holdingRecord.symbol === "string"
          ? holdingRecord.symbol
          : "";
      const tradeDate =
        parentRecord && typeof parentRecord.occurredOn === "string"
          ? parentRecord.occurredOn
          : "";
      const groupId = tradeDate ? `${holdingPath}:${parts[4]}` : holdingPath;
      const title = tradeDate
        ? `${name} · ${tradeDate}`
        : symbol && !name.includes(symbol) && !symbol.startsWith("CASH-")
          ? `${name}（${symbol}）`
          : name;
      recordPersonal(
        groupId,
        title,
        fieldLabel,
        formatPersonalValue(
          field,
          rawValue,
          parentRecord ?? holdingRecord,
          baseCurrency,
        ),
      );
      return undefined;
    }

    // Group financial summary / profile / portfolio totals.
    if (
      (parts[0] === "facts" && parts[1] === "summary") ||
      parts[0] === "financialProfile" ||
      parts[0] === "portfolio"
    ) {
      recordPersonal(
        "personal:summary",
        "个人财务摘要",
        fieldLabel,
        formatPersonalValue(field, rawValue, parentRecord, baseCurrency),
      );
      return undefined;
    }

    recordPersonal(
      `personal:${parts[0]}`,
      labels[parts[0]] ?? parts[0],
      fieldLabel,
      formatPersonalValue(field, rawValue, parentRecord, baseCurrency),
    );
    return undefined;
  };

  const report = trace?.structuredReport;
  report?.facts.forEach((claim, index) => {
    if (claim.basis === "user_data") {
      const refs = (claim.sourceRefs ?? [])
        .map((path) => processLocal(path))
        .filter((id): id is string => !!id);
      if (refs.length) localByFact.set(index, refs);
    }
  });
  (report && "inferences" in report ? report.inferences : []).forEach(
    (claim, index) => {
      if (claim.basis === "user_data") {
        const refs = (claim.sourceRefs ?? [])
          .map((path) => processLocal(path))
          .filter((id): id is string => !!id);
        if (refs.length) localByInference.set(index, refs);
      }
    },
  );
  // Older reports may provide references only through the evidence review.
  for (const review of trace?.adviceGrounding ?? []) {
    for (const check of review.checks) {
      if (
        check.sourceKind !== "local" ||
        report?.facts[check.factIndex]?.basis !== "user_data"
      )
        continue;
      const id = processLocal(check.sourceRef, check.quote);
      if (!id) continue;
      const refs = localByFact.get(check.factIndex) ?? [];
      if (!refs.includes(id)) refs.push(id);
      localByFact.set(check.factIndex, refs);
    }
  }
  report?.facts.forEach((claim, index) => {
    for (const id of [
      ...claim.evidenceIds,
      ...(localByFact.get(index) ?? []),
    ]) {
      const source = catalog.get(id);
      if (source) used.set(id, source);
    }
  });
  (report && "inferences" in report ? report.inferences : []).forEach(
    (claim, index) => {
      for (const id of [
        ...claim.evidenceIds,
        ...(localByInference.get(index) ?? []),
      ]) {
        const source = catalog.get(id);
        if (source) used.set(id, source);
      }
    },
  );

  const personalFacts: PersonalFactSummary[] = [...personalGroups.entries()]
    .map(([id, group]) => ({
      id,
      title: group.title,
      detail: [...group.entries.entries()]
        .map(([label, value]) => `${label} ${value}`)
        .join(" · "),
    }))
    .filter((item) => item.detail.length > 0);

  return {
    sources: [...used.values()],
    personalFacts,
    localByFact,
    localByInference,
  };
}

function sourceUrl(value: string) {
  try {
    const url = new URL(value);
    return ["https:", "http:"].includes(url.protocol) &&
      !url.username &&
      !url.password
      ? url.href
      : undefined;
  } catch {
    return undefined;
  }
}

function originalBody(source: AnalysisEvidenceReference) {
  if (!source.evidenceType?.startsWith("historical_")) return source.claim;
  try {
    const content = JSON.parse(source.claim ?? "");
    if (source.evidenceType === "historical_ai") return content.answer;
  } catch {
    // Older records may store plain text rather than a serialized answer.
  }
  return source.claim;
}

export function SourceDetail({
  source,
  regionId,
  close,
}: {
  source: AnalysisEvidenceReference;
  regionId: string;
  close: () => void;
}) {
  const region = useRef<HTMLElement>(null);
  const [error, setError] = useState("");
  const [downloading, setDownloading] = useState(false);
  useEffect(() => {
    setError("");
  }, [source.id]);
  const url = sourceUrl(source.sourceUrl);
  const download = async (id: string) => {
    if (downloading) return;
    setDownloading(true);
    setError("");
    try {
      const file = await getResearchAttachment(source.id, id);
      await saveAttachmentDownload(file);
    } catch (error) {
      setError(String(error));
    } finally {
      setDownloading(false);
    }
  };
  return (
    <section
      className="report-source-detail"
      id={regionId}
      role="region"
      aria-label="引用原始信息"
      tabIndex={-1}
      ref={region}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          close();
        }
      }}
    >
      <div className="report-source-heading">
        <h3>{source.title || "原始资料"}</h3>
      </div>
      {source.publisher && <p className="muted">{source.publisher}</p>}
      {source.asOfDate && <p className="muted">资料日期：{source.asOfDate}</p>}
      {source.capturedAt && (
        <p className="muted">保存或采集时间：{source.capturedAt}</p>
      )}
      {source.evidenceType === "native_web_summary" && (
        <p className="source-qualification">
          供应商生成的带引用摘要，需打开网页核对原文。
        </p>
      )}
      {source.evidenceType === "native_web_excerpt" && (
        <p className="source-qualification">供应商返回的网页原文片段。</p>
      )}
      {source.evidenceType === "historical_ai" && (
        <p className="source-qualification">
          历史模型回答，仅作为线索，未经结果验证。
        </p>
      )}
      <div className="report-original-text">
        {originalBody(source) || "这条旧记录未保存原始内容。"}
      </div>
      {source.notes && <p className="source-qualification">{source.notes}</p>}
      {url && (
        <ExternalLink href={url} target="_blank" rel="noreferrer">
          打开来源网页：{source.title || "原始来源"}
        </ExternalLink>
      )}
      {!!source.attachments?.length && (
        <ul aria-label="原始附件">
          {source.attachments.map((file) => (
            <li key={file.id}>
              <button
                type="button"
                className="text-button"
                disabled={downloading}
                onClick={() => void download(file.id)}
              >
                下载 {file.name}
              </button>
              <p className="source-qualification">{file.extractionNote}</p>
            </li>
          ))}
        </ul>
      )}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
