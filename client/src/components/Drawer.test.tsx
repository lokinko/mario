// @vitest-environment jsdom
import { useState, StrictMode } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Drawer, useDrawerControls } from "./Drawer";
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
function SavedForm() {
  const drawer = useDrawerControls();
  return (
    <>
      <input aria-label="名称" autoFocus />
      <button onClick={drawer.markClean}>保存</button>
      <button onClick={() => drawer.setBusy(true)}>开始保存</button>
    </>
  );
}
function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>打开</button>
      {open && (
        <Drawer title="资产详情" onClose={() => setOpen(false)}>
          <SavedForm />
        </Drawer>
      )}
    </>
  );
}
it("isolates background, restores focus and scroll, including StrictMode", () => {
  const { container } = render(
    <StrictMode>
      <Harness />
    </StrictMode>,
  );
  const trigger = screen.getByText("打开");
  trigger.focus();
  fireEvent.click(trigger);
  expect(container.inert).toBe(true);
  expect(document.body.style.overflow).toBe("hidden");
  expect(document.body.dataset.drawerOpen).toBe("true");
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(container.inert).not.toBe(true);
  expect(document.activeElement).toBe(trigger);
  expect(document.body.style.overflow).toBe("");
  expect(document.body.dataset.drawerOpen).toBeUndefined();
});
it("confirms discard, preserves cancelled drafts, and clears dirty state after save", () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  render(<Harness />);
  fireEvent.click(screen.getByText("打开"));
  fireEvent.change(screen.getByLabelText("名称"), {
    target: { value: "未保存" },
  });
  fireEvent.keyDown(window, { key: "Escape" });
  expect(confirm).toHaveBeenCalledOnce();
  expect(screen.getByDisplayValue("未保存")).toBeTruthy();
  fireEvent.click(screen.getByText("保存"));
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(confirm).toHaveBeenCalledOnce();
});
it("does not dismiss an in-flight nested transaction", () => {
  render(<Harness />);
  fireEvent.click(screen.getByText("打开"));
  fireEvent.click(screen.getByText("开始保存"));
  fireEvent.keyDown(window, { key: "Escape" });
  expect(screen.getByRole("dialog")).toBeTruthy();
  expect(
    (screen.getByLabelText("关闭详情") as HTMLButtonElement).disabled,
  ).toBe(true);
});
it("traps keyboard focus at both ends", () => {
  vi.spyOn(HTMLElement.prototype, "getClientRects").mockReturnValue([
    {},
  ] as unknown as DOMRectList);
  render(<Harness />);
  fireEvent.click(screen.getByText("打开"));
  const close = screen.getByLabelText("关闭详情"),
    last = screen.getByText("开始保存");
  close.focus();
  fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
  expect(document.activeElement).toBe(last);
  fireEvent.keyDown(window, { key: "Tab" });
  expect(document.activeElement).toBe(close);
});
it("ignores presentation filters when deciding whether to discard", () => {
  const confirm = vi.spyOn(window, "confirm");
  const close = vi.fn();
  render(
    <Drawer title="列表" onClose={close}>
      <div data-presentation>
        <input aria-label="搜索" />
      </div>
    </Drawer>,
  );
  fireEvent.change(screen.getByLabelText("搜索"), {
    target: { value: "筛选" },
  });
  fireEvent.keyDown(window, { key: "Escape" });
  expect(close).toHaveBeenCalledOnce();
  expect(confirm).not.toHaveBeenCalled();
});
