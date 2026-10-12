// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ServerData } from "./ServerData";
import * as accountApi from "../../api/account";

vi.mock("../../api/account", () => ({
  exportData: vi.fn(),
  importData: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

function mount() {
  const onRestore = vi.fn().mockResolvedValue(undefined);
  const flash = vi.fn();
  render(
    <ServerData
      flash={flash}
      onRestore={onRestore}
      autoState={{ phase: "synced", message: "已保存到服务器" }}
      onRetryAuto={vi.fn()}
    />,
  );
  return { onRestore, flash };
}
function chooseFile(value: string, size = value.length) {
  fireEvent.change(screen.getByLabelText("导入到空账号"), {
    target: { files: [{ size, text: () => Promise.resolve(value) }] },
  });
}

it("restores export and import controls after an export failure", async () => {
  vi.mocked(accountApi.exportData).mockRejectedValue(new Error("网络不可用"));
  mount();
  fireEvent.click(screen.getByRole("button", { name: "导出我的数据" }));
  expect(
    (screen.getByLabelText("导入到空账号") as HTMLInputElement).disabled,
  ).toBe(true);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "网络不可用",
  );
  expect(
    (screen.getByRole("button", { name: "导出我的数据" }) as HTMLButtonElement)
      .disabled,
  ).toBe(false);
  expect(
    (screen.getByLabelText("导入到空账号") as HTMLInputElement).disabled,
  ).toBe(false);
});

it("blocks oversized and malformed imports before confirmation or writing", async () => {
  const confirm = vi.spyOn(window, "confirm");
  mount();
  chooseFile("{}", 64 * 1024 * 1024 + 1);
  expect((await screen.findByRole("alert")).textContent).toContain("64 MB");
  chooseFile("invalid json");
  await waitFor(() =>
    expect(screen.getByRole("alert").textContent).not.toContain("64 MB"),
  );
  expect(confirm).not.toHaveBeenCalled();
  expect(accountApi.importData).not.toHaveBeenCalled();
});

it("cancels an import without writing or refreshing and allows another selection", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(false);
  const { onRestore, flash } = mount();
  chooseFile("{}");
  await waitFor(() => expect(window.confirm).toHaveBeenCalled());
  await waitFor(() =>
    expect(
      (screen.getByLabelText("导入到空账号") as HTMLInputElement).disabled,
    ).toBe(false),
  );
  expect(accountApi.importData).not.toHaveBeenCalled();
  expect(onRestore).not.toHaveBeenCalled();
  expect(flash).not.toHaveBeenCalled();
});

it("refreshes application data only after a confirmed import succeeds", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(accountApi.importData).mockResolvedValue(undefined);
  const { onRestore, flash } = mount();
  chooseFile('{"profile":{}}');
  await waitFor(() => expect(flash).toHaveBeenCalledWith("数据已导入"));
  expect(accountApi.importData).toHaveBeenCalledWith({ profile: {} });
  expect(onRestore).toHaveBeenCalledOnce();
});
