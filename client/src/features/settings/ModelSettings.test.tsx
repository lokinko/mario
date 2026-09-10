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
