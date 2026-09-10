// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Foundation } from "./Foundation";
import * as api from "../../api";
import type { Snapshot } from "../../types";
import emptySnapshot from "../../test/emptySnapshot.json";
vi.mock("../../api", async (original) => ({
  ...(await original<typeof import("../../api")>()),
  saveProfile: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
it("keeps financial inputs visible and reports failed saves", async () => {
  vi.mocked(api.saveProfile).mockRejectedValue(new Error("保存失败"));
  const onUpdate = vi.fn();
  render(
    <Foundation
      snapshot={emptySnapshot as Snapshot}
      onUpdate={onUpdate}
      flash={vi.fn()}
    />,
  );
  const input = screen.getByRole("spinbutton", { name: "月收入 CNY" });
  fireEvent.change(input, { target: { value: "12345" } });
  fireEvent.click(screen.getByRole("button", { name: "保存并检查" }));
  expect((await screen.findByRole("alert")).textContent).toContain("保存失败");
  expect(screen.getByDisplayValue("12345")).toBeTruthy();
  expect(onUpdate).not.toHaveBeenCalled();
});
