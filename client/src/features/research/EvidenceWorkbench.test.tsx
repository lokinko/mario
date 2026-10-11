// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import * as api from "../../api";
import type { ResearchEvidence } from "../../types";
import { EvidenceWorkbench } from "./EvidenceWorkbench";

vi.mock("../../api", () => ({
  getResearchEvidence: vi.fn(),
  getResearchAttachment: vi.fn(),
  saveResearchEvidence: vi.fn(),
  setResearchEvidenceStatus: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

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

it("saves complete source text with one field and clears it after success", async () => {
  vi.mocked(api.getResearchEvidence).mockResolvedValue([]);
  vi.mocked(api.saveResearchEvidence).mockResolvedValue(saved);
  render(<EvidenceWorkbench flash={vi.fn()} navigate={vi.fn()} />);
  const input = screen.getByRole("textbox", { name: "原始资料与资讯" });
  expect(screen.getAllByRole("textbox")).toHaveLength(1);
  expect(screen.queryByRole("combobox")).toBeNull();
  fireEvent.change(input, { target: { value: saved.claim } });
  fireEvent.click(screen.getByRole("button", { name: "保存资料" }));
  await waitFor(() =>
    expect(api.saveResearchEvidence).toHaveBeenCalledWith({
      content: saved.claim,
    }),
  );
  await waitFor(() => expect((input as HTMLTextAreaElement).value).toBe(""));
});

it("blocks blank content and keeps the draft when saving fails", async () => {
  vi.mocked(api.getResearchEvidence).mockResolvedValue([]);
  vi.mocked(api.saveResearchEvidence)
    .mockRejectedValueOnce(new Error("保存失败"))
    .mockResolvedValue(saved);
  render(<EvidenceWorkbench flash={vi.fn()} navigate={vi.fn()} />);
  const input = screen.getByRole("textbox", { name: "原始资料与资讯" });
  fireEvent.change(input, { target: { value: " \n " } });
  const button = screen.getByRole("button", {
    name: "保存资料",
  }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  fireEvent.submit(button.closest("form")!);
  expect(api.saveResearchEvidence).not.toHaveBeenCalled();
  fireEvent.change(input, { target: { value: saved.claim } });
  fireEvent.click(button);
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect((input as HTMLTextAreaElement).value).toBe(saved.claim);
  fireEvent.click(screen.getByRole("button", { name: "保存资料" }));
  await waitFor(() => expect((input as HTMLTextAreaElement).value).toBe(""));
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
  vi.mocked(api.getResearchEvidence)
    .mockResolvedValueOnce([saved, legacy])
    .mockResolvedValue([{ ...saved, active: false }, legacy]);
  vi.mocked(api.setResearchEvidenceStatus).mockResolvedValue({
    ...saved,
    active: false,
  });
  render(<EvidenceWorkbench flash={vi.fn()} navigate={vi.fn()} />);
  await screen.findAllByText("保存于 2026-10-11");
  const texts = document.querySelectorAll(".evidence-content");
  expect(texts[0].textContent).toBe(saved.claim);
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
    expect(texts[1].textContent).toContain(value);
  }
  fireEvent.click(screen.getAllByRole("button", { name: "归档" })[0]);
  await waitFor(() =>
    expect(api.setResearchEvidenceStatus).toHaveBeenCalledWith("text", false),
  );
  expect(await screen.findByRole("button", { name: "恢复" })).toBeTruthy();
});

it("carries source topics into the question so saved text can be retrieved", async () => {
  vi.mocked(api.getResearchEvidence).mockResolvedValue([
    { ...saved, title: "黄金储备变化" },
    { ...saved, id: "archived", title: "旧资料", active: false },
  ]);
  const navigate = vi.fn();
  render(<EvidenceWorkbench flash={vi.fn()} navigate={navigate} />);
  await screen.findAllByText(/保存于/);
  fireEvent.click(screen.getByRole("button", { name: "用资料提问" }));
  const question = window.sessionStorage.getItem("mario.advisorQuestion");
  expect(question).toContain("黄金储备变化");
  expect(question).not.toContain("旧资料");
  expect(navigate).toHaveBeenCalledWith("advisor");
});

it("saves attachments without requiring text and preserves them for retry", async () => {
  vi.mocked(api.getResearchEvidence).mockResolvedValue([]);
  vi.mocked(api.saveResearchEvidence)
    .mockRejectedValueOnce(new Error("上传失败"))
    .mockResolvedValue(saved);
  render(<EvidenceWorkbench flash={vi.fn()} navigate={vi.fn()} />);
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
  const submission = vi.mocked(api.saveResearchEvidence).mock.calls[0][0];
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
  vi.mocked(api.getResearchEvidence).mockResolvedValue([]);
  render(<EvidenceWorkbench flash={vi.fn()} navigate={vi.fn()} />);
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
  expect(api.saveResearchEvidence).not.toHaveBeenCalled();
});

it("downloads the original file through the authenticated API", async () => {
  vi.mocked(api.getResearchEvidence).mockResolvedValue([
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
  vi.mocked(api.getResearchAttachment).mockResolvedValue({
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
  render(<EvidenceWorkbench flash={vi.fn()} navigate={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "报告.txt" }));
  await waitFor(() =>
    expect(api.getResearchAttachment).toHaveBeenCalledWith("text", "file"),
  );
  await waitFor(() => expect(downloaded).toBe("报告.txt"));
  expect(createObjectURL).toHaveBeenCalledOnce();
});
