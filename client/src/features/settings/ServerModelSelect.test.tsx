// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ServerModelSelect } from "./ServerModelSelect";
import * as api from "../../api/settings";
vi.mock("../../api/settings", () => ({ getServerModels: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("automatically fetches the catalog and preserves the selected model until the user changes it", async () => {
  vi.mocked(api.getServerModels).mockResolvedValue({
    models: ["gpt-5.4", "gpt-5.4-mini"],
    defaultModel: "gpt-5.4",
  });
  const change = vi.fn();
  render(
    <ServerModelSelect
      value="saved-model"
      onChange={change}
      defaultModel="gpt-5.4"
      disabled={false}
    />,
  );
  await screen.findByRole("option", { name: "gpt-5.4-mini" });
  expect(api.getServerModels).toHaveBeenCalledTimes(1);
  expect((screen.getByLabelText("站点模型") as HTMLSelectElement).value).toBe(
    "saved-model",
  );
  expect(
    screen.getByRole("option", { name: "gpt-5.4 · 站点默认" }),
  ).toBeTruthy();
  expect(change).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("站点模型"), {
    target: { value: "gpt-5.4-mini" },
  });
  expect(change).toHaveBeenCalledWith("gpt-5.4-mini");
});

it("keeps current and default choices when loading fails, and can retry", async () => {
  vi.mocked(api.getServerModels)
    .mockRejectedValueOnce(new Error("offline"))
    .mockResolvedValueOnce({
      models: ["new-model"],
      defaultModel: "default-model",
    });
  render(
    <ServerModelSelect
      value="saved-model"
      onChange={vi.fn()}
      defaultModel="default-model"
      disabled={false}
    />,
  );
  await screen.findByText(/模型目录暂时无法读取/);
  expect((screen.getByLabelText("站点模型") as HTMLSelectElement).value).toBe(
    "saved-model",
  );
  expect(
    screen.getByRole("option", { name: "default-model · 站点默认" }),
  ).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "重新获取模型" }));
  await screen.findByRole("option", { name: "new-model" });
  expect(screen.queryByText(/模型目录暂时无法读取/)).toBeNull();
});

it("ignores a response after unmount and locks selection while saving", async () => {
  let finish!: (result: api.ServerModelCatalog) => void;
  vi.mocked(api.getServerModels).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const view = render(
    <ServerModelSelect
      value="saved"
      onChange={vi.fn()}
      defaultModel="default"
      disabled
    />,
  );
  expect(
    (screen.getByLabelText("站点模型") as HTMLSelectElement).disabled,
  ).toBe(true);
  view.unmount();
  vi.mocked(api.getServerModels).mockResolvedValue({
    models: ["fresh-model"],
    defaultModel: "default",
  });
  render(
    <ServerModelSelect
      value="default"
      onChange={vi.fn()}
      defaultModel="default"
      disabled={false}
    />,
  );
  await screen.findByRole("option", { name: "fresh-model" });
  finish({ models: ["late-model"], defaultModel: "old-default" });
  await waitFor(() =>
    expect(screen.queryByRole("option", { name: "late-model" })).toBeNull(),
  );
});
