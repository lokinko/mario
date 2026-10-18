// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { CodexModelSelect } from "./CodexModelSelect";
import { ModelSettings } from "./ModelSettings";
import * as api from "../../api/settings";
vi.mock("../../api/settings", () => ({
  getCodexBridgeStatus: vi.fn(),
  saveModelConfig: vi.fn(),
}));
vi.mock("../../api/portfolio", () => ({
  getSecurityPriceConfig: vi.fn(async () => null),
}));
vi.mock("./AnalysisDefaults", () => ({ AnalysisDefaults: () => null }));
const models = [
  {
    model: "model-a",
    displayName: "Model A",
    description: "General tasks",
    isDefault: true,
  },
  {
    model: "model-b",
    displayName: "Model B",
    description: "Complex tasks",
    isDefault: false,
  },
];
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
it("preserves a nondefault selection through status updates and allows switching", () => {
  const change = vi.fn();
  const props = {
    value: "model-b",
    onChange: change,
    bridgeStatus: { paired: true, online: true, model: "model-a", models },
  };
  const view = render(<CodexModelSelect {...props} />);
  expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe(
    "model-b",
  );
  expect(screen.getByText("Complex tasks")).toBeTruthy();
  view.rerender(
    <CodexModelSelect
      {...props}
      bridgeStatus={{ ...props.bridgeStatus, models: [...models] }}
    />,
  );
  expect(change).not.toHaveBeenCalled();
  fireEvent.change(screen.getByRole("combobox"), {
    target: { value: "model-a" },
  });
  expect(change).toHaveBeenCalledWith("model-a");
});
it("keeps the saved model visible when disconnected or removed from the catalog", () => {
  const props = {
    value: "old-model",
    onChange: vi.fn(),
    bridgeStatus: null,
  };
  const view = render(<CodexModelSelect {...props} />);
  expect((screen.getByRole("combobox") as HTMLSelectElement).disabled).toBe(
    true,
  );
  expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe(
    "old-model",
  );
  view.rerender(
    <CodexModelSelect
      {...props}
      bridgeStatus={{ paired: true, online: true, model: "model-a", models }}
    />,
  );
  expect((screen.getByRole("combobox") as HTMLSelectElement).disabled).toBe(
    false,
  );
  expect(screen.getByText("old-model（已保存，待确认可用）")).toBeTruthy();
});
it("saves the selected bridge model rather than the computer default", async () => {
  const config = {
    provider: "codex-bridge" as const,
    baseUrl: "codex://bridge",
    model: "model-a",
    hasApiKey: true,
  };
  vi.mocked(api.getCodexBridgeStatus).mockResolvedValue({
    paired: true,
    online: true,
    model: "model-a",
    models,
  });
  vi.mocked(api.saveModelConfig).mockResolvedValue({
    ...config,
    model: "model-b",
  });
  const update = vi.fn();
  render(<ModelSettings model={config} onUpdate={update} flash={vi.fn()} />);
  await screen.findByText("Model B");
  fireEvent.change(screen.getByRole("combobox", { name: "模型名称" }), {
    target: { value: "model-b" },
  });
  fireEvent.click(screen.getByText("保存配置"));
  await waitFor(() =>
    expect(api.saveModelConfig).toHaveBeenCalledWith({
      provider: "codex-bridge",
      baseUrl: "codex://bridge",
      model: "model-b",
      apiKey: undefined,
    }),
  );
  expect(update).toHaveBeenCalledWith({ ...config, model: "model-b" });
});
