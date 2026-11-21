// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { dividendCard } from "./card";
import { DividendShare } from "./DividendShare";
import { createDividendPoster } from "./poster";
import snapshot from "./snapshot.json";

const stock = snapshot.stocks.find(({ symbol }) => symbol === "600900");
if (!stock) throw new Error("Missing sample stock 600900");
const sampleCard = dividendCard(stock)!;

type MockContext = {
  fillRect: ReturnType<typeof vi.fn>;
  clearRect: ReturnType<typeof vi.fn>;
  scale: ReturnType<typeof vi.fn>;
  save: ReturnType<typeof vi.fn>;
  restore: ReturnType<typeof vi.fn>;
  beginPath: ReturnType<typeof vi.fn>;
  roundRect: ReturnType<typeof vi.fn>;
  fill: ReturnType<typeof vi.fn>;
  stroke: ReturnType<typeof vi.fn>;
  fillText: ReturnType<typeof vi.fn>;
  drawImage: ReturnType<typeof vi.fn>;
  measureText: ReturnType<typeof vi.fn>;
  fillStyle: string;
  strokeStyle: string;
  lineWidth: number;
  font: string;
  textBaseline: string;
  textAlign: string;
  imageSmoothingEnabled: boolean;
};

let contexts: MockContext[] = [];
let createObjectURL: ReturnType<typeof vi.spyOn>;
let revokeObjectURL: ReturnType<typeof vi.spyOn>;
let blobCounter = 0;

function makeContext(): MockContext {
  return {
    fillRect: vi.fn(),
    clearRect: vi.fn(),
    scale: vi.fn(),
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    roundRect: vi.fn(),
    fill: vi.fn(),
    stroke: vi.fn(),
    fillText: vi.fn(),
    drawImage: vi.fn(),
    measureText: vi.fn((text: string) => ({ width: text.length * 20 })),
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 1,
    font: "",
    textBaseline: "alphabetic",
    textAlign: "left",
    imageSmoothingEnabled: true,
  };
}

beforeEach(() => {
  contexts = [];
  blobCounter = 0;
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(() => {
    const ctx = makeContext();
    contexts.push(ctx);
    return ctx as unknown as CanvasRenderingContext2D;
  });
  vi.spyOn(HTMLCanvasElement.prototype, "toBlob").mockImplementation(
    function (callback) {
      callback(new Blob(["png"], { type: "image/png" }));
    },
  );
  createObjectURL = vi
    .spyOn(URL, "createObjectURL")
    .mockImplementation(() => `blob:poster-${++blobCounter}`);
  revokeObjectURL = vi
    .spyOn(URL, "revokeObjectURL")
    .mockImplementation(() => {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("omits QR code and scan caption in poster when qr canvas is not provided", async () => {
  const blob = await createDividendPoster(sampleCard);
  expect(blob.type).toBe("image/png");
  const ctx = contexts.at(-1)!;
  expect(ctx.drawImage).not.toHaveBeenCalled();
  expect(
    ctx.fillText.mock.calls.some(([value]) => value === "扫码查看详情"),
  ).toBe(false);
});

it("draws QR code and scan caption in poster when qr canvas is provided", async () => {
  const qrCanvas = document.createElement("canvas");
  const blob = await createDividendPoster(sampleCard, qrCanvas);
  expect(blob.type).toBe("image/png");
  const ctx = contexts.at(-1)!;
  expect(ctx.drawImage).toHaveBeenCalledTimes(1);
  expect(ctx.drawImage.mock.calls[0][0]).toBe(qrCanvas);
  expect(
    ctx.fillText.mock.calls.some(([value]) => value === "扫码查看详情"),
  ).toBe(true);
});

it("defaults to exporting without QR code and toggles QR code on demand without triggering draft confirmation", async () => {
  const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
  const onDismiss = vi.fn();

  render(<DividendShare card={sampleCard} onDismiss={onDismiss} />);

  const checkbox = screen.getByRole("checkbox", {
    name: "导出时附带二维码",
  }) as HTMLInputElement;
  expect(checkbox.checked).toBe(false);

  const initialImage = (await screen.findByRole("img")) as HTMLImageElement;
  expect(initialImage.src).toBe("blob:poster-1");
  expect(initialImage.alt).not.toContain("二维码");
  expect(document.querySelector(".dividend-share-qr")).toBeNull();
  expect(contexts.every((ctx) => ctx.drawImage.mock.calls.length === 0)).toBe(
    true,
  );

  fireEvent.click(checkbox);
  expect(checkbox.checked).toBe(true);

  await waitFor(() => {
    const updatedImage = screen.getByRole("img") as HTMLImageElement;
    expect(updatedImage.src).toBe("blob:poster-2");
    expect(updatedImage.alt).toContain("详情二维码");
  });
  expect(revokeObjectURL).toHaveBeenCalledWith("blob:poster-1");
  expect(contexts.some((ctx) => ctx.drawImage.mock.calls.length === 1)).toBe(
    true,
  );

  fireEvent.click(checkbox);
  expect(checkbox.checked).toBe(false);

  await waitFor(() => {
    const revertedImage = screen.getByRole("img") as HTMLImageElement;
    expect(revertedImage.src).toBe("blob:poster-3");
    expect(revertedImage.alt).not.toContain("二维码");
  });
  expect(revokeObjectURL).toHaveBeenCalledWith("blob:poster-2");
  expect(createObjectURL).toHaveBeenCalledTimes(3);

  fireEvent.click(screen.getByRole("button", { name: "关闭详情" }));
  expect(confirmSpy).not.toHaveBeenCalled();
  expect(onDismiss).toHaveBeenCalledTimes(1);
});
