// @vitest-environment jsdom
import { lazy } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { DeferredPage } from "./DeferredPage";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
it("keeps the surrounding UI usable while loading the selected page", async () => {
  let finish!: (value: { default: () => React.JSX.Element }) => void;
  const Page = lazy(
    () =>
      new Promise<{ default: () => React.JSX.Element }>((resolve) => {
        finish = resolve;
      }),
  );
  render(
    <>
      <button>其他页面</button>
      <DeferredPage>
        <Page />
      </DeferredPage>
    </>,
  );
  expect(screen.getByRole("status", { name: "正在加载" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "其他页面" })).toBeTruthy();
  finish({ default: () => <p>页面已加载</p> });
  expect(await screen.findByText("页面已加载")).toBeTruthy();
});
it("offers a deliberate reload after failure and preserves inputs when cancelled", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const Page = lazy(() =>
    Promise.reject(new Error("synthetic import failure")),
  );
  const reload = vi.fn();
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  render(
    <DeferredPage onReload={reload}>
      <Page />
    </DeferredPage>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "重新加载" }));
  expect(reload).not.toHaveBeenCalled();
  confirm.mockReturnValue(true);
  fireEvent.click(screen.getByRole("button", { name: "重新加载" }));
  expect(reload).toHaveBeenCalledOnce();
});
