import { httpRequest, httpStreamRequest } from "../lib/httpClient";
import type {
  AnalysisRequest,
  AnalysisPreview,
  AnalysisResult,
  AnalysisHistoryItem,
  ResearchEvidence,
  ResearchEvidenceInput,
  StoredAnalysis,
} from "../types";
export async function runAnalysis(
  request: AnalysisRequest,
  onUpdate?: (
    update:
      { type: "draft"; answer: string } | { type: "status"; message: string },
  ) => void,
  signal?: AbortSignal,
): Promise<AnalysisResult> {
  if (onUpdate)
    return httpStreamRequest<AnalysisResult>(
      "/analysis/stream",
      {
        method: "POST",
        body: JSON.stringify(request),
        signal,
      },
      (event) => {
        if (!event || typeof event !== "object")
          throw new Error("流式回复格式无效");
        const update = event as Record<string, unknown>;
        if (update.type === "draft" && typeof update.answer === "string")
          onUpdate({ type: "draft", answer: update.answer });
        else if (update.type === "status" && typeof update.message === "string")
          onUpdate({ type: "status", message: update.message });
        else if (update.type === "error" && typeof update.message === "string")
          throw new Error(update.message);
        else if (
          update.type === "complete" &&
          update.result &&
          typeof update.result === "object"
        ) {
          const result = update.result as AnalysisResult;
          if (
            typeof result.id !== "string" ||
            typeof result.answer !== "string"
          )
            throw new Error("完整回复格式无效");
          return result;
        } else throw new Error("流式回复格式无效");
        return undefined;
      },
    );
  return httpRequest<AnalysisResult>("/analysis", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function getAnalysisHistory(): Promise<AnalysisHistoryItem[]> {
  return httpRequest<AnalysisHistoryItem[]>("/analyses");
}

export async function getAnalysis(id: string): Promise<StoredAnalysis> {
  return httpRequest<StoredAnalysis>(`/analyses/${encodeURIComponent(id)}`);
}

export async function previewAnalysis(
  request: AnalysisRequest,
): Promise<AnalysisPreview> {
  return httpRequest<AnalysisPreview>("/analysis/preview", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function getResearchEvidence(): Promise<ResearchEvidence[]> {
  return httpRequest<ResearchEvidence[]>("/research-evidence");
}

export async function saveResearchEvidence(
  evidence: ResearchEvidenceInput,
): Promise<ResearchEvidence> {
  return httpRequest<ResearchEvidence>("/research-evidence", {
    method: "POST",
    body: JSON.stringify(evidence),
  });
}

export async function getResearchAttachment(
  evidenceId: string,
  attachmentId: string,
): Promise<{ name: string; dataBase64: string }> {
  return httpRequest(
    `/research-evidence/${encodeURIComponent(evidenceId)}/attachments/${encodeURIComponent(attachmentId)}`,
  );
}

export async function setResearchEvidenceStatus(
  id: string,
  active: boolean,
): Promise<ResearchEvidence> {
  return httpRequest<ResearchEvidence>(
    `/research-evidence/${encodeURIComponent(id)}/status`,
    { method: "PUT", body: JSON.stringify({ active }) },
  );
}
