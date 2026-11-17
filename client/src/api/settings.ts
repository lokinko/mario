import { httpRequest } from "../lib/httpClient";
import type {
  AnalysisSettings,
  ModelConfig,
  ModelConnectionTest,
} from "../types/ai";
export async function getModelConfig(): Promise<ModelConfig> {
  return httpRequest<ModelConfig>("/model-config");
}

export type ServerModelCatalog = { models: string[]; defaultModel: string };
export function getServerModels(): Promise<ServerModelCatalog> {
  return httpRequest<ServerModelCatalog>("/model-config/models");
}

export async function saveModelConfig(
  config: Omit<ModelConfig, "hasApiKey" | "serverModel"> & { apiKey?: string },
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

export type CodexBridgeStatus = {
  paired: boolean;
  online: boolean;
  model: string | null;
  models?: CodexModel[];
};
export type CodexModel = {
  model: string;
  displayName: string;
  description: string;
  isDefault: boolean;
};
export function getCodexBridgeStatus() {
  return httpRequest<CodexBridgeStatus>("/codex-bridge");
}
export function pairCodexBridge() {
  return httpRequest<{ pairingCode: string; expiresInSeconds: number }>(
    "/codex-bridge",
    { method: "POST" },
  );
}
export function enableCodexBridge() {
  return httpRequest<ModelConfig>("/codex-bridge/enable", { method: "POST" });
}
export function revokeCodexBridge() {
  return httpRequest<{ ok: boolean }>("/codex-bridge", { method: "DELETE" });
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
