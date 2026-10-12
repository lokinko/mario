import { httpRequest } from "../lib/httpClient";
import type {
  AnalysisSettings,
  ModelConfig,
  ModelConnectionTest,
} from "../types";
export async function getModelConfig(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config");
}

export async function saveModelConfig(
  config: Omit<ModelConfig, "hasApiKey"> & { apiKey?: string },
): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config", {
    method: "PUT",
    body: JSON.stringify(config),
  });
}

export async function testModelConnection(): Promise<ModelConnectionTest> {
  return httpRequest<ModelConnectionTest>("/model-config/test", {
    method: "POST",
  });
}

export async function deleteModelKey(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-key", { method: "DELETE" });
}

export async function readCodexCredentials(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config/codex", { method: "POST" });
}

export async function getAnalysisSettings(): Promise<AnalysisSettings> {
  return httpRequest<AnalysisSettings>("/analysis-settings");
}

export async function saveAnalysisSettings(
  settings: AnalysisSettings,
): Promise<AnalysisSettings> {
  return httpRequest<AnalysisSettings>("/analysis-settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}
