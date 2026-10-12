import { httpRequest } from "../lib/httpClient";
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
): Promise<AnalysisResult> {
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
