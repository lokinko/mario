// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AnalysisDefaults } from "./AnalysisDefaults";
import * as api from "../../api";
import { analysisSettings } from "../../test/analysisSettings";

vi.mock("../../api", () => ({
  getAnalysisSettings: vi.fn(),
  saveAnalysisSettings: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
const model = {
  provider: "openai-responses" as const,
  baseUrl: "",
  model: "test",
  hasApiKey: true,
};

it("loads saved permissions and saves one preset for future questions", async () => {
  const stored = {
    ...analysisSettings,
    contextSelection: {
      ...analysisSettings.contextSelection,
      includeProfile: false,
    },
  };
  vi.mocked(api.getAnalysisSettings).mockResolvedValue(stored);
  vi.mocked(api.saveAnalysisSettings).mockImplementation(
    async (settings) => settings,
  );
  const flash = vi.fn();
  render(<AnalysisDefaults model={model} flash={flash} />);
  const profile = (await screen.findByLabelText(
    "财务档案",
  )) as HTMLInputElement;
  expect(profile.checked).toBe(false);
  fireEvent.click(screen.getByLabelText("持仓明细"));
  fireEvent.click(screen.getByLabelText("参考长期记忆"));
  fireEvent.click(screen.getByLabelText("联网查找相关资料"));
  fireEvent.click(screen.getByRole("button", { name: "保存问答预设" }));
  await waitFor(() => expect(flash).toHaveBeenCalled());
  expect(api.saveAnalysisSettings).toHaveBeenCalledWith({
    ...stored,
    useMemory: false,
    webSearch: false,
    contextSelection: { ...stored.contextSelection, includeHoldings: false },
  });
});

it("keeps deep-only options while switching to quick analysis", async () => {
  vi.mocked(api.getAnalysisSettings).mockResolvedValue(analysisSettings);
  render(
    <AnalysisDefaults
      model={{ ...model, provider: "openai-chat" }}
      flash={vi.fn()}
    />,
  );
  fireEvent.change(await screen.findByLabelText("分析方式"), {
    target: { value: "quick" },
  });
  expect(
    (screen.getByLabelText("参考长期记忆") as HTMLInputElement).disabled,
  ).toBe(true);
  expect(screen.getByText(/当前 Chat Completions 接口不支持/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("分析方式"), {
    target: { value: "deep" },
  });
  const memory = screen.getByLabelText("参考长期记忆") as HTMLInputElement;
  expect(memory.disabled).toBe(false);
  expect(memory.checked).toBe(true);
});

it("shows a read failure and retries without overwriting saved permissions", async () => {
  vi.mocked(api.getAnalysisSettings)
    .mockRejectedValueOnce(new Error("无法读取预设"))
    .mockResolvedValue(analysisSettings);
  render(<AnalysisDefaults model={model} flash={vi.fn()} />);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "无法读取预设",
  );
  expect(
    (screen.getByRole("button", { name: "保存问答预设" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "重试" }));
  await screen.findByLabelText("财务档案");
  expect(api.saveAnalysisSettings).not.toHaveBeenCalled();
});

it("preserves edited selections when saving fails", async () => {
  vi.mocked(api.getAnalysisSettings).mockResolvedValue(analysisSettings);
  vi.mocked(api.saveAnalysisSettings).mockRejectedValue(new Error("保存失败"));
  const flash = vi.fn();
  render(<AnalysisDefaults model={model} flash={flash} />);
  fireEvent.click(await screen.findByLabelText("财务档案"));
  fireEvent.click(screen.getByRole("button", { name: "保存问答预设" }));
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect((screen.getByLabelText("财务档案") as HTMLInputElement).checked).toBe(
    false,
  );
  expect(flash).not.toHaveBeenCalled();
});
