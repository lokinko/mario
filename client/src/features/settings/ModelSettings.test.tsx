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
import * as settingsApi from "../../api/settings";

vi.mock("../../api/settings", () => ({
  getAnalysisSettings: vi.fn().mockResolvedValue(null),
  saveAnalysisSettings: vi.fn(),
  saveModelConfig: vi
    .fn()
    .mockImplementation(async (input) => ({ ...input, hasApiKey: true })),
  deleteModelKey: vi.fn(),
  testModelConnection: vi.fn(),
}));
vi.mock("../../api/portfolio", () => ({
  getSecurityPriceConfig: vi.fn().mockResolvedValue({ hasApiKey: false }),
  deleteSecurityPriceKey: vi.fn(),
  getSecurityPrice: vi.fn(),
  saveSecurityPriceConfig: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("saves Anthropic with its endpoint and replacement key", async () => {
  const onUpdate = vi.fn();
  render(
    <ModelSettings
      model={{
        provider: "openai-responses",
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
  expect(settingsApi.saveModelConfig).toHaveBeenCalledWith({
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

it("saves a third-party Chat Completions connection with a custom URL, port and model", async () => {
  const onUpdate = vi.fn();
  render(
    <ModelSettings model={existingModel} onUpdate={onUpdate} flash={vi.fn()} />,
  );
  fireEvent.change(screen.getByLabelText("模型接口"), {
    target: { value: "openai-chat" },
  });
  fireEvent.change(screen.getByLabelText("API Base URL", { exact: false }), {
    target: { value: "http://gateway.example.test:8080/custom/v1" },
  });
  fireEvent.change(screen.getByLabelText("模型名称"), {
    target: { value: "third-party-model" },
  });
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "third-party-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalled());
  expect(settingsApi.saveModelConfig).toHaveBeenCalledWith({
    provider: "openai-chat",
    baseUrl: "http://gateway.example.test:8080/custom/v1",
    model: "third-party-model",
    apiKey: "third-party-key",
  });
  expect((screen.getByLabelText("API Key") as HTMLInputElement).value).toBe("");
});

it("offers Codex only through the paired bridge", () => {
  render(
    <ModelSettings model={existingModel} onUpdate={vi.fn()} flash={vi.fn()} />,
  );
  const options = [
    ...(screen.getByLabelText("模型接口") as HTMLSelectElement).options,
  ].map((option) => option.value);
  expect(options).toEqual([
    "openai-chat",
    "openai-responses",
    "anthropic",
    "codex-bridge",
  ]);
});

it("locks configuration and key writes during a connection test and releases them on failure", async () => {
  let reject!: (error: Error) => void;
  vi.mocked(settingsApi.testModelConnection).mockReturnValue(
    new Promise((_, fail) => {
      reject = fail;
    }),
  );
  render(
    <ModelSettings model={existingModel} onUpdate={vi.fn()} flash={vi.fn()} />,
  );
  fireEvent.click(screen.getByRole("button", { name: "测试已保存连接" }));
  expect(
    (screen.getByRole("button", { name: "保存配置" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(
    (screen.getByRole("button", { name: "移除密钥" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(
    (screen.getByLabelText("模型接口") as HTMLSelectElement).matches(
      ":disabled",
    ),
  ).toBe(true);
  reject(new Error("连接失败"));
  expect((await screen.findByRole("alert")).textContent).toContain("连接失败");
  expect(
    (screen.getByRole("button", { name: "保存配置" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
  expect(settingsApi.saveModelConfig).not.toHaveBeenCalled();
  expect(settingsApi.deleteModelKey).not.toHaveBeenCalled();
});
