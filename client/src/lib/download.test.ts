// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { saveAttachmentDownload, saveTextDownload } from "./download";
afterEach(() => {
  vi.clearAllMocks();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
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
  expect(document.querySelector("a")).toBeNull();
  expect(revokeObjectURL).not.toHaveBeenCalled();
  vi.advanceTimersByTime(1000);
  expect(revokeObjectURL).toHaveBeenCalledWith("blob:download");
});

it("preserves binary attachments and UTF-8 CSV with its BOM in browser downloads", async () => {
  const blobs: Blob[] = [];
  const originalURL = URL;
  vi.stubGlobal(
    "URL",
    class extends originalURL {
      static createObjectURL = (blob: Blob) => {
        blobs.push(blob);
        return "blob:download";
      };
      static revokeObjectURL = vi.fn();
    },
  );
  const readBlob = (blob: Blob) =>
    new Promise<ArrayBuffer>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as ArrayBuffer);
      reader.onerror = () => reject(reader.error);
      reader.readAsArrayBuffer(blob);
    });
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
  const bytes = new Uint8Array([0, 255, 128, 10]);
  await saveAttachmentDownload({
    name: "报告.pdf",
    dataBase64: btoa(String.fromCharCode(...bytes)),
  });
  await saveTextDownload("模板.csv", "\ufeff资产,金额\n存款,10", "text/csv");
  expect(new Uint8Array(await readBlob(blobs[0]))).toEqual(bytes);
  const csv = new Uint8Array(await readBlob(blobs[1]));
  expect(Array.from(csv).slice(0, 3)).toEqual([239, 187, 191]);
  expect(new TextDecoder().decode(csv)).toBe("资产,金额\n存款,10");
  expect(blobs[1].type).toBe("text/csv");
});
