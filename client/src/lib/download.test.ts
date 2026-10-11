// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import {
  saveAttachmentDownload,
  saveDownload,
  saveTextDownload,
} from "./download";
import { save } from "@tauri-apps/plugin-dialog";
import { writeFile } from "@tauri-apps/plugin-fs";

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));
vi.mock("@tauri-apps/plugin-fs", () => ({ writeFile: vi.fn() }));
afterEach(() => {
  vi.clearAllMocks();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});
function native() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
}

it.each([
  "/Users/test/报告.pdf",
  "C:\\Users\\test\\报告.pdf",
  "content://documents/42",
])(
  "writes exact binary attachment bytes to the selected native destination %s",
  async (path) => {
    native();
    vi.mocked(save).mockResolvedValue(path);
    vi.mocked(writeFile).mockResolvedValue(undefined);
    const bytes = new Uint8Array([0, 255, 128, 10]);
    expect(
      await saveAttachmentDownload({
        name: "报告.pdf",
        dataBase64: btoa(String.fromCharCode(...bytes)),
      }),
    ).toBe(true);
    expect(save).toHaveBeenCalledWith({
      defaultPath: "报告.pdf",
      title: "保存文件",
    });
    expect(writeFile).toHaveBeenCalledWith(path, bytes);
    expect(document.querySelector("a")).toBeNull();
  },
);

it("does not write when the user cancels the native save dialog", async () => {
  native();
  vi.mocked(save).mockResolvedValue(null);
  expect(await saveDownload("取消.txt", new Uint8Array([1]))).toBe(false);
  expect(writeFile).not.toHaveBeenCalled();
});

it("preserves UTF-8 text and propagates native write failure to the caller", async () => {
  native();
  vi.mocked(save).mockResolvedValue("content://documents/csv");
  vi.mocked(writeFile).mockRejectedValue(new Error("空间不足"));
  await expect(
    saveTextDownload("模板.csv", "\ufeff资产,金额\n存款,10", "text/csv"),
  ).rejects.toThrow("空间不足");
  const bytes = vi.mocked(writeFile).mock.calls[0][1];
  expect(Array.from(bytes as Uint8Array).slice(0, 3)).toEqual([239, 187, 191]);
  expect(new TextDecoder().decode(bytes as Uint8Array)).toBe(
    "资产,金额\n存款,10",
  );
});

it("uses browser downloads on Web and releases the object URL after the click", async () => {
  vi.useFakeTimers();
  const createObjectURL = vi.fn(() => "blob:download");
  const revokeObjectURL = vi.fn();
  const originalURL = URL;
  vi.stubGlobal(
    "URL",
    class extends originalURL {
      static createObjectURL = createObjectURL;
      static revokeObjectURL = revokeObjectURL;
    },
  );
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, "click")
    .mockImplementation(function (this: HTMLAnchorElement) {
      expect(this.isConnected).toBe(true);
      expect(this.download).toBe("备份.json");
    });
  await saveTextDownload("备份.json", "{}", "application/json");
  expect(click).toHaveBeenCalledOnce();
  expect(save).not.toHaveBeenCalled();
  expect(writeFile).not.toHaveBeenCalled();
  expect(document.querySelector("a")).toBeNull();
  expect(revokeObjectURL).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1000);
  expect(revokeObjectURL).toHaveBeenCalledWith("blob:download");
});
