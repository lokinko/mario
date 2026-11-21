// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import * as researchApi from "../../api/evidence";
import type { ResearchEvidence } from "../../types/user";
import { EvidenceWorkbench } from "./EvidenceWorkbench";

vi.mock("../../api/evidence", () => ({
  getResearchEvidence: vi.fn(),
  getResearchAttachment: vi.fn(),
  saveResearchEvidence: vi.fn(),
  setResearchEvidenceStatus: vi.fn(),
  setResearchEvidenceTags: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const stocks = [
  { name: "招商银行", symbol: "600036.SS" },
  { name: "长江电力", symbol: "600900.SS" },
];

const saved: ResearchEvidence = {
  id: "text",
  assetName: "",
  title: "原始资料",
  publisher: "用户保存的原始资料",
  sourceUrl: "",
  sourceTier: "用户资料（待核实）",
  evidenceType: "user_source_text",
  stance: "背景",
  asOfDate: "",
  claim: "  原始信息：\n\n营收同比变化。\n来源 https://example.com/report\n  ",
  notes: "",
  active: true,
  capturedAt: "2026-10-11T01:00:00Z",
};

it("saves complete source text with optional stock and type tags and clears it after success", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    { ...saved, id: "typed", category: "年报" },
  ]);
  vi.mocked(researchApi.saveResearchEvidence).mockResolvedValue(saved);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  await screen.findByRole("button", { name: "原始资料" });
  fireEvent.click(screen.getByRole("button", { name: "新增资料" }));
  const input = screen.getByRole("textbox", { name: "原始资料与资讯" });
  const stock = screen.getByLabelText("关联股票") as HTMLSelectElement;
  expect(Array.from(stock.options).map((option) => option.text)).toEqual([
    "不关联",
    "招商银行（600036.SS）",
    "长江电力（600900.SS）",
  ]);
  expect(
    Array.from(document.querySelectorAll("#evidence-categories option")).map(
      (option) => option.getAttribute("value"),
    ),
  ).toEqual(["年报"]);
  fireEvent.change(input, { target: { value: saved.claim } });
  fireEvent.change(stock, { target: { value: "长江电力 600900.SS" } });
  fireEvent.change(screen.getByLabelText("信息类型"), {
    target: { value: "公告" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存资料" }));
  await waitFor(() =>
    expect(researchApi.saveResearchEvidence).toHaveBeenCalledWith({
      content: saved.claim,
      assetName: "长江电力 600900.SS",
      category: "公告",
    }),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "新增资料" }));
  expect(
    (
      screen.getByRole("textbox", {
        name: "原始资料与资讯",
      }) as HTMLTextAreaElement
    ).value,
  ).toBe("");
});

it("blocks blank content and keeps the draft when saving fails", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([]);
  vi.mocked(researchApi.saveResearchEvidence)
    .mockRejectedValueOnce(new Error("保存失败"))
    .mockResolvedValue(saved);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "新增资料" }));
  const input = screen.getByRole("textbox", { name: "原始资料与资讯" });
  fireEvent.change(input, { target: { value: " \n " } });
  const button = screen.getByRole("button", {
    name: "保存资料",
  }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  fireEvent.submit(button.closest("form")!);
  expect(researchApi.saveResearchEvidence).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: saved.claim } });
  fireEvent.click(button);
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect((input as HTMLTextAreaElement).value).toBe(saved.claim);
  fireEvent.click(screen.getByRole("button", { name: "保存资料" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "新增资料" }));
  expect(
    (
      screen.getByRole("textbox", {
        name: "原始资料与资讯",
      }) as HTMLTextAreaElement
    ).value,
  ).toBe("");
});

it("shows original text and all legacy information, with reversible archiving", async () => {
  const legacy: ResearchEvidence = {
    ...saved,
    id: "legacy",
    assetName: "旧资产",
    title: "旧报告",
    publisher: "旧发布方",
    sourceUrl: "https://example.com/legacy",
    sourceTier: "媒体报道",
    evidenceType: "新闻",
    stance: "反驳",
    asOfDate: "2026-06-30",
    claim: "旧资料全文",
    notes: "原有限制",
  };
  vi.mocked(researchApi.getResearchEvidence)
    .mockResolvedValueOnce([saved, legacy])
    .mockResolvedValue([{ ...saved, active: false }, legacy]);
  vi.mocked(researchApi.setResearchEvidenceStatus).mockResolvedValue({
    ...saved,
    active: false,
  });
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "原始资料" }));
  expect(screen.getByText("保存于 2026-10-11")).toBeTruthy();
  expect(document.querySelector(".evidence-content")?.textContent).toBe(
    saved.claim,
  );
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  fireEvent.click(screen.getByRole("button", { name: "旧报告" }));
  for (const value of [
    legacy.assetName,
    legacy.title,
    legacy.publisher,
    legacy.sourceUrl,
    legacy.sourceTier,
    legacy.evidenceType,
    legacy.stance,
    legacy.asOfDate,
    legacy.claim,
    legacy.notes,
  ]) {
    expect(document.querySelector(".evidence-content")?.textContent).toContain(
      value,
    );
  }
  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  fireEvent.click(screen.getByRole("button", { name: "原始资料" }));
  fireEvent.click(screen.getByRole("button", { name: "归档" }));
  await waitFor(() =>
    expect(researchApi.setResearchEvidenceStatus).toHaveBeenCalledWith(
      "text",
      false,
    ),
  );
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  const row = screen.getByRole("button", { name: "原始资料" });
  expect(within(row).getByText("已归档")).toBeTruthy();
  fireEvent.click(row);
  expect(await screen.findByRole("button", { name: "恢复" })).toBeTruthy();
});

it("asks AI directly about one selected evidence item from its drawer", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    { ...saved, title: "黄金储备变化", assetName: "招商银行 600036.SS" },
    { ...saved, id: "archived", title: "旧资料", active: false },
  ]);
  const onAsk = vi.fn();
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={onAsk} />);
  expect(screen.queryByRole("button", { name: "用资料提问" })).toBeNull();
  fireEvent.click(await screen.findByRole("button", { name: "黄金储备变化" }));
  fireEvent.click(screen.getByRole("button", { name: "基于此资料提问" }));
  const question = onAsk.mock.calls[0][0];
  expect(question).toContain("黄金储备变化");
  expect(question).toContain("招商银行 600036.SS");
  expect(question).not.toContain("旧资料");
  expect(onAsk).toHaveBeenCalledTimes(1);
});

it("saves attachments without requiring text and preserves them for retry", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([]);
  vi.mocked(researchApi.saveResearchEvidence)
    .mockRejectedValueOnce(new Error("上传失败"))
    .mockResolvedValue(saved);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "新增资料" }));
  const file = new File(["原文\n第二段"], "报告.txt", { type: "text/plain" });
  fireEvent.change(screen.getByLabelText("选择附件"), {
    target: { files: [file] },
  });
  const button = screen.getByRole("button", {
    name: "保存资料",
  }) as HTMLButtonElement;
  expect(button.disabled).toBe(false);
  fireEvent.click(button);
  expect((await screen.findByRole("alert")).textContent).toContain("上传失败");
  expect(screen.getByText("报告.txt")).toBeTruthy();
  const submission = vi.mocked(researchApi.saveResearchEvidence).mock
    .calls[0][0];
  expect(submission.content).toBe("");
  expect(submission.attachments?.[0].name).toBe("报告.txt");
  expect(
    new TextDecoder().decode(
      Uint8Array.from(atob(submission.attachments![0].dataBase64), (c) =>
        c.charCodeAt(0),
      ),
    ),
  ).toBe("原文\n第二段");
  fireEvent.click(screen.getByRole("button", { name: "保存资料" }));
  await waitFor(() => expect(screen.queryByText("报告.txt")).toBeNull());
});

it("can remove a selected file and rejects too many or oversized attachments", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([]);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(screen.getByRole("button", { name: "新增资料" }));
  const picker = screen.getByLabelText("选择附件");
  fireEvent.change(picker, {
    target: { files: [new File(["report"], "报告.txt")] },
  });
  fireEvent.click(screen.getByRole("button", { name: "移除附件 报告.txt" }));
  expect(screen.queryByText("报告.txt")).toBeNull();
  expect(
    (screen.getByRole("button", { name: "保存资料" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.change(picker, {
    target: {
      files: Array.from({ length: 6 }, (_, i) => new File(["a"], `${i}.txt`)),
    },
  });
  expect((await screen.findByRole("alert")).textContent).toContain(
    "最多附带 5 个文件",
  );
  fireEvent.change(picker, {
    target: {
      files: [new File([new Uint8Array(5 * 1024 * 1024 + 1)], "大文件.pdf")],
    },
  });
  expect(screen.getByRole("alert").textContent).toContain("不能超过 5 MB");
  expect(researchApi.saveResearchEvidence).not.toHaveBeenCalled();
});

it("downloads the original file through the authenticated API", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    {
      ...saved,
      attachments: [
        {
          id: "file",
          name: "报告.txt",
          byteSize: 8,
          extractionNote: "已提取文字，可用于问答",
        },
      ],
    },
  ]);
  vi.mocked(researchApi.getResearchAttachment).mockResolvedValue({
    name: "报告.txt",
    dataBase64: btoa("original"),
  });
  const createObjectURL = vi.fn(() => "blob:attachment");
  const revokeObjectURL = vi.fn();
  const originalURL = URL;
  vi.stubGlobal(
    "URL",
    class extends originalURL {
      static createObjectURL = createObjectURL;
      static revokeObjectURL = revokeObjectURL;
    },
  );
  let downloaded = "";
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
    this: HTMLAnchorElement,
  ) {
    downloaded = this.download;
  });
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "原始资料" }));
  fireEvent.click(await screen.findByRole("button", { name: "报告.txt" }));
  await waitFor(() =>
    expect(researchApi.getResearchAttachment).toHaveBeenCalledWith(
      "text",
      "file",
    ),
  );
  await waitFor(() => expect(downloaded).toBe("报告.txt"));
  expect(createObjectURL).toHaveBeenCalledOnce();
});

it("lists every saved item as one file row, newest first, with archived ones marked", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    {
      ...saved,
      id: "old",
      title: "旧资料",
      capturedAt: "2026-09-01T01:00:00Z",
    },
    {
      ...saved,
      id: "archived",
      title: "归档资料",
      active: false,
      capturedAt: "2026-10-01T01:00:00Z",
    },
    {
      ...saved,
      id: "new",
      title: "新资料",
      capturedAt: "2026-11-01T01:00:00Z",
    },
  ]);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  await screen.findByRole("button", { name: "新资料" });
  const rows = within(screen.getByRole("region", { name: "资料档案" }))
    .getAllByRole("button")
    .map((row) => row.getAttribute("aria-label"));
  expect(rows).toEqual(["新资料", "归档资料", "旧资料"]);
  expect(
    screen.getByRole("button", { name: "归档资料" }).closest("li")?.className,
  ).toBe("inactive");
});

it("filters saved evidence by keyword, stock, category, and archive status", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    {
      ...saved,
      id: "cmb",
      title: "招行年报摘录",
      assetName: "招商银行 600036.SS",
      category: "年报",
      claim: "分红率提升至 35%",
      capturedAt: "2026-11-02T01:00:00Z",
    },
    {
      ...saved,
      id: "cypc",
      title: "长电公告",
      assetName: "长江电力 600900.SS",
      category: "公告",
      claim: "来水偏丰",
      active: false,
      capturedAt: "2026-11-01T01:00:00Z",
    },
  ]);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  await screen.findByRole("button", { name: "招行年报摘录" });
  fireEvent.change(screen.getByRole("searchbox", { name: "搜索参考资料" }), {
    target: { value: "分红率" },
  });
  expect(screen.getByRole("button", { name: "招行年报摘录" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "长电公告" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "清除筛选" }));
  fireEvent.change(screen.getByRole("combobox", { name: "按归档状态筛选" }), {
    target: { value: "archived" },
  });
  expect(screen.getByRole("button", { name: "长电公告" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "招行年报摘录" })).toBeNull();
  fireEvent.change(screen.getByRole("combobox", { name: "按归档状态筛选" }), {
    target: { value: "all" },
  });
  fireEvent.change(screen.getByRole("combobox", { name: "按关联股票筛选" }), {
    target: { value: "招商银行 600036.SS" },
  });
  expect(screen.getByRole("button", { name: "招行年报摘录" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: "长电公告" })).toBeNull();
});

it("edits tags after saving without touching the original text", async () => {
  const tagged = {
    ...saved,
    assetName: "招商银行 600036.SS",
    category: "年报",
  };
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([tagged]);
  vi.mocked(researchApi.setResearchEvidenceTags).mockResolvedValue({
    ...tagged,
    assetName: "",
    category: "公告",
  });
  const flash = vi.fn();
  render(<EvidenceWorkbench stocks={stocks} flash={flash} onAsk={vi.fn()} />);
  const row = await screen.findByRole("button", { name: "原始资料" });
  expect(within(row).getByText("招商银行")).toBeTruthy();
  expect(within(row).getByText("年报")).toBeTruthy();
  fireEvent.click(row);
  const save = screen.getByRole("button", {
    name: "保存标签",
  }) as HTMLButtonElement;
  expect(save.disabled).toBe(true);
  fireEvent.change(screen.getByLabelText("关联股票"), {
    target: { value: "" },
  });
  fireEvent.change(screen.getByLabelText("信息类型"), {
    target: { value: "公告" },
  });
  fireEvent.click(save);
  await waitFor(() =>
    expect(researchApi.setResearchEvidenceTags).toHaveBeenCalledWith("text", {
      assetName: "",
      category: "公告",
    }),
  );
  await waitFor(() => expect(flash).toHaveBeenCalledWith("标签已更新"));
  expect(document.querySelector(".evidence-content")?.textContent).toBe(
    saved.claim,
  );
  expect(within(row).getByText("公告")).toBeTruthy();
  expect(within(row).queryByText("招商银行")).toBeNull();
});

it("keeps a stock tag that is no longer held or watched selectable", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    { ...saved, assetName: "已卖出公司 000001.SZ" },
  ]);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "原始资料" }));
  expect((screen.getByLabelText("关联股票") as HTMLSelectElement).value).toBe(
    "已卖出公司 000001.SZ",
  );
});

it("matches attachment filenames case-insensitively and resets empty multi-filter states", async () => {
  vi.mocked(researchApi.getResearchEvidence).mockResolvedValue([
    {
      ...saved,
      id: "att-item",
      title: "海外财报摘要",
      assetName: "招商银行 600036.SS",
      category: "年报",
      active: true,
      attachments: [
        {
          id: "a1",
          name: "FY2026_Annual_Report.PDF",
          byteSize: 1024,
          extractionNote: "已提取",
        },
      ],
    },
    {
      ...saved,
      id: "arch-item",
      title: "历史归档纪要",
      assetName: "",
      category: "公告",
      active: false,
      attachments: [],
    },
  ]);
  render(<EvidenceWorkbench stocks={stocks} flash={vi.fn()} onAsk={vi.fn()} />);
  expect(
    await screen.findByRole("button", { name: "海外财报摘要" }),
  ).toBeTruthy();

  fireEvent.change(screen.getByLabelText("搜索参考资料"), {
    target: { value: "annual_report.pdf" },
  });
  await waitFor(() => {
    expect(screen.getByRole("button", { name: "海外财报摘要" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "历史归档纪要" })).toBeNull();
  });

  // Combine with archived status so nothing matches, then clear all filters in one click.
  fireEvent.change(screen.getByLabelText("按归档状态筛选"), {
    target: { value: "archived" },
  });
  expect(await screen.findByText(/没有符合当前筛选条件的资料/)).toBeTruthy();
  fireEvent.click(screen.getAllByRole("button", { name: "清除筛选" })[0]);
  expect(
    await screen.findByRole("button", { name: "海外财报摘要" }),
  ).toBeTruthy();
  expect(screen.getByRole("button", { name: "历史归档纪要" })).toBeTruthy();
});
