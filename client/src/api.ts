import type {
  AnalysisRequest,
  AnalysisResult,
  DecisionEntry,
  FinancialProfile,
  Goal,
  Holding,
  ModelConfig,
  Snapshot,
} from "./types";

const API_BASE = import.meta.env.VITE_API_URL ?? "http://127.0.0.1:4217/api";

async function httpRequest<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response | undefined;
  let lastError: unknown;
  const attempts = !init?.method || init.method === "GET" ? 10 : 1;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      response = await fetch(`${API_BASE}${path}`, {
        ...init,
        headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
      });
      break;
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => window.setTimeout(resolve, 150));
    }
  }
  if (!response) throw lastError ?? new Error("无法连接本地服务");
  if (!response.ok) {
    const body = await response.text();
    throw new Error(body || `本地服务返回 ${response.status}`);
  }
  if (response.status === 204) return undefined as T;
  return response.json() as Promise<T>;
}

export async function getSnapshot(): Promise<Snapshot> {
  return httpRequest<Snapshot>("/snapshot");
}

export async function saveProfile(profile: FinancialProfile): Promise<Snapshot> {
  return httpRequest<Snapshot>("/profile", { method: "PUT", body: JSON.stringify(profile) });
}

export async function saveHolding(holding: Omit<Holding, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>("/holdings", { method: "POST", body: JSON.stringify(holding) });
}

export async function saveGoal(goal: Omit<Goal, "id">): Promise<Snapshot> {
  return httpRequest<Snapshot>("/goals", { method: "POST", body: JSON.stringify(goal) });
}

export async function getModelConfig(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config");
}

export async function saveModelConfig(config: Omit<ModelConfig, "hasApiKey"> & { apiKey?: string }): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config", { method: "PUT", body: JSON.stringify(config) });
}

export async function runAnalysis(request: AnalysisRequest): Promise<AnalysisResult> {
  return httpRequest<AnalysisResult>("/analysis", { method: "POST", body: JSON.stringify(request) });
}

export async function saveDecision(entry: DecisionEntry): Promise<void> {
  await httpRequest<void>("/decisions", { method: "POST", body: JSON.stringify(entry) });
}
