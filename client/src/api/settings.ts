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
export function getCodexModels() {
  return httpRequest<CodexModel[]>("/model-config/codex/models");
}
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
