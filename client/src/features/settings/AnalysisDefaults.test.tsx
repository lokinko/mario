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
import * as settingsApi from "../../api/settings";
import { analysisSettings } from "../../test/analysisSettings";

vi.mock("../../api/settings", () => ({
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
  vi.mocked(settingsApi.getAnalysisSettings).mockResolvedValue(stored);
  vi.mocked(settingsApi.saveAnalysisSettings).mockImplementation(
    async (settings) => settings,
  );
  const flash = vi.fn();
  render(<AnalysisDefaults model={model} flash={flash} />);
  const profile = (await screen.findByLabelText(
    "财务档案",
  )) as HTMLInputElement;
  expect(profile.checked).toBe(false);
  fireEvent.click(screen.getByLabelText("持仓明细"));
  fireEvent.click(screen.getByLabelText("联网查找相关资料"));
  fireEvent.click(screen.getByRole("button", { name: "保存问答预设" }));
  await waitFor(() => expect(flash).toHaveBeenCalled());
  expect(settingsApi.saveAnalysisSettings).toHaveBeenCalledWith({
    ...stored,
    webSearch: false,
    contextSelection: { ...stored.contextSelection, includeHoldings: false },
  });
});

it("offers only the three supported data groups and always uses deep analysis", async () => {
  vi.mocked(settingsApi.getAnalysisSettings).mockResolvedValue(
    analysisSettings,
  );
  render(
    <AnalysisDefaults
      model={{ ...model, provider: "openai-chat" }}
      flash={vi.fn()}
    />,
  );
  await screen.findByLabelText("财务档案");
  for (const label of ["财务档案", "持仓明细", "研究证据与历史搜索资料"]) {
    expect((screen.getByLabelText(label) as HTMLInputElement).checked).toBe(
      true,
    );
  }
  for (const label of [
    "规划结果",
    "风险检查",
    "个人规则",
    "周期复盘",
    "每日资产记录",
  ]) {
    expect(screen.queryByLabelText(label)).toBeNull();
  }
  expect(screen.queryByRole("combobox")).toBeNull();
  expect(screen.queryByText("快速分析")).toBeNull();
  for (const label of ["反思检查", "比较备选方案"]) {
    expect((screen.getByLabelText(label) as HTMLInputElement).disabled).toBe(
      false,
    );
  }
  expect(screen.getByText(/当前 Chat Completions 接口不支持/)).toBeTruthy();
});

it("shows a read failure and retries without overwriting saved permissions", async () => {
  vi.mocked(settingsApi.getAnalysisSettings)
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
  expect(settingsApi.saveAnalysisSettings).not.toHaveBeenCalled();
});

it("preserves edited selections when saving fails", async () => {
  vi.mocked(settingsApi.getAnalysisSettings).mockResolvedValue(
    analysisSettings,
  );
  vi.mocked(settingsApi.saveAnalysisSettings).mockRejectedValue(
    new Error("保存失败"),
  );
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
