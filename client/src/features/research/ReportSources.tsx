import { ExternalLink } from "../../components/ExternalLink";
import { useEffect, useRef, useState } from "react";
import { saveAttachmentDownload } from "../../lib/download";
import { getResearchAttachment } from "../../api";
import type {
  AnalysisEvidenceReference,
  AnalysisWorkflowTrace,
} from "../../types";

const labels: Record<string, string> = {
  financialProfile: "我的收支",
  portfolio: "我的持仓",
  goals: "我的目标",
  currentUserMessage: "本轮原始消息",
  dailyAssetHistory: "每日资产记录",
  deterministicPlan: "规划结果",
  riskFindings: "风险检查",
  personalInvestmentRules: "投资规则",
  periodicSystemReviews: "原始复盘",
  portfolioEvents: "原始流水",
  marketValue: "市值",
  costBasis: "成本",
  monthlyIncome: "月收入",
  monthlyExpense: "月支出",
  emergencyFund: "备用金",
  emergencyMonths: "备用金月数",
  debtBalance: "负债",
  concentrationPct: "集中度",
  totalValue: "资产总额",
  targetAmount: "目标金额",
  targetDate: "目标日期",
  content: "原始文字",
  amount: "金额",
  note: "备注",
  valuationDate: "估值日期",
  currency: "币种",
};

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

// A source appears only when the final report actually references it. Selecting
// a candidate is not itself a citation, and internal model metadata is omitted.
export function reportSources(trace?: AnalysisWorkflowTrace) {
  const catalog = new Map(
    (trace?.evidenceCatalog ?? []).map((source) => [source.id, source]),
  );
  const used = new Map<string, AnalysisEvidenceReference>();
  const localByFact = new Map<number, string[]>();
  const localByInference = new Map<number, string[]>();
  const addLocal = (path: string, quote?: string) => {
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
    const id = `local:${path}`;
    const parts = path.split("/").slice(1);
    const parent = pointer(
      trace?.personalContext,
      path.slice(0, path.lastIndexOf("/")),
    );
    const name =
      parent && typeof parent === "object" && "name" in parent
        ? String(parent.name)
        : "";
    const title = [
      labels[parts[0]],
      name,
      parts.length > 1 ? labels[parts.at(-1)!] : undefined,
    ]
      .filter(Boolean)
      .join(" · ");
    catalog.set(id, {
      id,
      title,
      publisher: "本次问答使用的个人资料",
      sourceUrl: "",
      sourceTier: "用户记录",
      asOfDate: "",
      evidenceType: "personal_snapshot",
      claim: quote ?? String(value),
    });
    return id;
  };
  const report = trace?.structuredReport;
  const referencedLocal = (path: string) => addLocal(path);
  report?.facts.forEach((claim, index) => {
    if (claim.basis === "user_data")
      localByFact.set(
        index,
        (claim.sourceRefs ?? [])
          .map(referencedLocal)
          .filter((id): id is string => !!id),
      );
  });
  report?.inferences.forEach((claim, index) => {
    if (claim.basis === "user_data")
      localByInference.set(
        index,
        (claim.sourceRefs ?? [])
          .map(referencedLocal)
          .filter((id): id is string => !!id),
      );
  });
  // Older reports may provide references only through the evidence review.
  for (const review of trace?.adviceGrounding ?? []) {
    for (const check of review.checks) {
      if (
        check.sourceKind !== "local" ||
        report?.facts[check.factIndex]?.basis !== "user_data"
      )
        continue;
      const id = addLocal(check.sourceRef, check.quote);
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
  report?.inferences.forEach((claim, index) => {
    for (const id of [
      ...claim.evidenceIds,
      ...(localByInference.get(index) ?? []),
    ]) {
      const source = catalog.get(id);
      if (source) used.set(id, source);
    }
  });
  return { sources: [...used.values()], localByFact, localByInference };
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
    region.current?.focus({ preventScroll: true });
    region.current?.scrollIntoView?.({ behavior: "smooth", block: "nearest" });
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
        <button type="button" className="text-button" onClick={close}>
          收起原始信息
        </button>
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
