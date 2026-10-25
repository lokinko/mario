// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Menu } from "./Menu";
afterEach(cleanup);
it("moves through enabled actions and restores focus on Escape without activating an action", () => {
  const action = vi.fn();
  render(
    <Menu
      label="持仓操作"
      items={[
        { label: "编辑", action },
        { label: "不可用", action, disabled: true },
        { label: "删除", action, danger: true },
      ]}
    />,
  );
  const trigger = screen.getByRole("button", { name: "持仓操作" });
  trigger.focus();
  fireEvent.keyDown(trigger, { key: "ArrowDown" });
  expect(document.activeElement).toBe(
    screen.getByRole("menuitem", { name: "编辑" }),
  );
  fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
  expect(document.activeElement).toBe(
    screen.getByRole("menuitem", { name: "删除" }),
  );
  fireEvent.keyDown(document.activeElement!, { key: "Escape" });
  expect(screen.queryByRole("menu")).toBeNull();
  expect(document.activeElement).toBe(trigger);
  expect(action).not.toHaveBeenCalled();
});
it("closes on outside interaction and performs one chosen action", () => {
  const action = vi.fn();
  render(
    <>
      <Menu label="操作" items={[{ label: "编辑", action }]} />
      <button>其他</button>
    </>,
  );
  fireEvent.click(screen.getByRole("button", { name: "操作" }));
  fireEvent.pointerDown(screen.getByRole("button", { name: "其他" }));
  expect(screen.queryByRole("menu")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "操作" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "编辑" }));
  expect(action).toHaveBeenCalledOnce();
  expect(screen.queryByRole("menu")).toBeNull();
});
