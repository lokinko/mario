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
  getServerModels: vi.fn().mockResolvedValue({
    models: ["site-model", "site-alt"],
    defaultModel: "site-model",
  }),
  fetchAccountModels: vi.fn().mockResolvedValue({
    models: ["existing-model", "gpt-5.4"],
  }),
  getAnalysisSettings: vi.fn().mockResolvedValue(null),
  saveAnalysisSettings: vi.fn(),
  saveModelConfig: vi
    .fn()
    .mockImplementation(async (input) => ({ ...input, hasApiKey: true })),
  deleteModelKey: vi.fn(),
  testModelConnection: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

it("fetches Anthropic models with a replacement key and saves the chosen model", async () => {
  vi.mocked(settingsApi.fetchAccountModels).mockResolvedValue({
    models: ["claude-fixture", "claude-alt"],
  });
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
  expect((screen.getByLabelText("模型名称") as HTMLSelectElement).value).toBe(
    "",
  );
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "test-only-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "获取可用模型" }));
  await screen.findByRole("option", { name: "claude-fixture" });
  expect(settingsApi.fetchAccountModels).toHaveBeenCalledWith({
    provider: "anthropic",
    baseUrl: "https://api.anthropic.com/v1",
    apiKey: "test-only-key",
  });
  fireEvent.change(screen.getByLabelText("模型名称"), {
    target: { value: "claude-fixture" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalled());
  expect(settingsApi.saveModelConfig).toHaveBeenCalledWith({
    source: "account",
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

it("saves a third-party Chat Completions connection with a custom URL, port and custom model name", async () => {
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
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "third-party-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "添加自定义模型" }));
  fireEvent.change(screen.getByLabelText("自定义模型名称"), {
    target: { value: "third-party-model" },
  });
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() => expect(onUpdate).toHaveBeenCalled());
  expect(settingsApi.saveModelConfig).toHaveBeenCalledWith({
    source: "account",
    provider: "openai-chat",
    baseUrl: "http://gateway.example.test:8080/custom/v1",
    model: "third-party-model",
    apiKey: "third-party-key",
  });
  expect((screen.getByLabelText("API Key") as HTMLInputElement).value).toBe("");
});

it("offers local ChatGPT OAuth only through the paired bridge", () => {
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

it("loads site models without a key form and saves the chosen model at the official destination", async () => {
  render(
    <ModelSettings
      model={{
        ...existingModel,
        source: "server",
        model: "site-model",
        serverModel: "site-model",
      }}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(screen.queryByLabelText("API Key")).toBeNull();
  expect(screen.queryByLabelText("API Base URL")).toBeNull();
  expect(screen.queryByRole("button", { name: "移除密钥" })).toBeNull();
  expect((screen.getByLabelText("站点模型") as HTMLSelectElement).value).toBe(
    "site-model",
  );
  await screen.findByRole("option", { name: "site-alt" });
  fireEvent.change(screen.getByLabelText("站点模型"), {
    target: { value: "site-alt" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() =>
    expect(settingsApi.saveModelConfig).toHaveBeenCalledWith({
      source: "server",
      provider: "openai-responses",
      baseUrl: "https://api.openai.com/v1",
      model: "site-alt",
      apiKey: undefined,
    }),
  );
});

it("switches between a personal model and the site model without sending a personal key to the site", async () => {
  render(
    <ModelSettings
      model={{ ...existingModel, source: "account", serverModel: "site-model" }}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("API Key"), {
    target: { value: "draft-personal-key" },
  });
  fireEvent.change(screen.getByLabelText("调用方式"), {
    target: { value: "server" },
  });
  expect(screen.queryByLabelText("API Key")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "保存配置" }));
  await waitFor(() =>
    expect(settingsApi.saveModelConfig).toHaveBeenCalledWith(
      expect.objectContaining({ source: "server", apiKey: undefined }),
    ),
  );
  await waitFor(() =>
    expect(
      (screen.getByRole("button", { name: "保存配置" }) as HTMLButtonElement)
        .disabled,
    ).toBe(false),
  );
  fireEvent.change(screen.getByLabelText("调用方式"), {
    target: { value: "account" },
  });
  expect((screen.getByLabelText("API Key") as HTMLInputElement).value).toBe("");
});

it("keeps a disabled site service visible and offers a personal model", () => {
  render(
    <ModelSettings
      model={{
        ...existingModel,
        source: "server",
        model: "",
        hasApiKey: false,
      }}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  expect(screen.getByText(/站点 OpenAI 暂未启用/)).toBeTruthy();
  expect(
    (
      screen.getByRole("button", {
        name: "测试已保存连接",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  expect(
    (screen.getByRole("button", { name: "保存配置" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.change(screen.getByLabelText("调用方式"), {
    target: { value: "account" },
  });
  expect(screen.getByLabelText("API Key")).toBeTruthy();
});

it("does not copy the site default model into personal model selection when switching from server", () => {
  render(
    <ModelSettings
      model={{
        ...existingModel,
        source: "server",
        model: "site-only-model",
        serverModel: "site-only-model",
      }}
      onUpdate={vi.fn()}
      flash={vi.fn()}
    />,
  );
  fireEvent.change(screen.getByLabelText("调用方式"), {
    target: { value: "account" },
  });
  expect((screen.getByLabelText("模型名称") as HTMLSelectElement).value).toBe(
    "",
  );
});
