// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ModelSettings } from "./ModelSettings";
import * as api from "../../api";

vi.mock("../../api", () => ({
  getSecurityPriceConfig: vi.fn().mockResolvedValue({ hasApiKey: false }),
  saveModelConfig: vi
    .fn()
    .mockImplementation(async (input) => ({ ...input, hasApiKey: true })),
  readCodexCredentials: vi.fn(),
  deleteModelKey: vi.fn(),
  deleteSecurityPriceKey: vi.fn(),
  getSecurityPrice: vi.fn(),
  saveSecurityPriceConfig: vi.fn(),
  testModelConnection: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("migrates the legacy selection and saves Anthropic with its endpoint and replacement key", async () => {
  const onUpdate = vi.fn();
  render(
    <ModelSettings
      model={{
        provider: "openai-compatible",
        baseUrl: "https://api.openai.com/v1",
        model: "existing-model",
        hasApiKey: true,
      }}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  expect((screen.getByLabelText("模型接口") as HTMLSelectElement).value).toBe(
    "openai-responses",
  );
  fireEvent.change(screen.getByLabelText("模型接口"), {
    target: { value: "anthropic" },
  });
  expect(
    (screen.getByLabelText("API Base URL") as HTMLInputElement).value,
  ).toBe("https://api.anthropic.com/v1");
  expect((screen.getByLabelText("模型名称") as HTMLInputElement).value).toBe(
    "",
  );
  fireEvent.change(screen.getByLabelText("模型名称"), {
    target: { value: "claude-fixture" },
  });
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "test-only-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalled());
  expect(api.saveModelConfig).toHaveBeenCalledWith({
    provider: "anthropic",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-fixture",
    apiKey: "test-only-key",
  });
  expect((screen.getByLabelText("API Key") as HTMLInputElement).value).toBe("");
});

const existingModel = {
  provider: "openai-responses" as const,
  baseUrl: "https://api.openai.com/v1",
  model: "existing-model",
  hasApiKey: true,
};

it("reads Codex in one click and selects its detected model without copying a key", async () => {
  const detected = {
    provider: "codex" as const,
    baseUrl: "codex://local",
    model: "detected-model",
    hasApiKey: true,
  };
  vi.mocked(api.readCodexCredentials).mockResolvedValue(detected);
  const onUpdate = vi.fn();
  render(
    <ModelSettings model={existingModel} onUpdate={onUpdate} flash={vi.fn()} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "一键读取 Codex 凭证" }));
  await screen.findByText(/Codex 凭证读取成功/);
  expect(onUpdate).toHaveBeenCalledWith(detected);
  expect((screen.getByLabelText("模型接口") as HTMLSelectElement).value).toBe(
    "codex",
  );
  expect((screen.getByLabelText("模型名称") as HTMLInputElement).value).toBe(
    "detected-model",
  );
  expect(screen.queryByLabelText("API Key")).toBeNull();
  expect(api.saveModelConfig).not.toHaveBeenCalled();
});

it("shows a missing-login failure and preserves the existing model", async () => {
  vi.mocked(api.readCodexCredentials).mockRejectedValue(
    new Error("未找到已登录的 Codex 凭证"),
  );
  const onUpdate = vi.fn();
  render(
    <ModelSettings model={existingModel} onUpdate={onUpdate} flash={vi.fn()} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "一键读取 Codex 凭证" }));
  await screen.findByText(/Codex 凭证读取失败.*未找到已登录/);
  expect(onUpdate).not.toHaveBeenCalled();
  expect((screen.getByLabelText("模型接口") as HTMLSelectElement).value).toBe(
    "openai-responses",
  );
  expect((screen.getByLabelText("模型名称") as HTMLInputElement).value).toBe(
    "existing-model",
  );
  expect(screen.queryByText(/Codex 凭证读取成功/)).toBeNull();
});
