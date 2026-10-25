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
import { saveTextDownload } from "../../lib/download";

vi.mock("../../api/account", () => ({
  exportData: vi.fn(),
  importData: vi.fn(),
  prepareDataReplacement: vi.fn(),
}));
vi.mock("../../lib/download", () => ({
  saveTextDownload: vi.fn().mockResolvedValue(undefined),
}));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

function mount() {
  const onRestore = vi.fn().mockResolvedValue(undefined);
  const flash = vi.fn();
  const view = render(
    <ServerData
      flash={flash}
      onRestore={onRestore}
      autoState={{ phase: "synced", message: "已保存到服务器" }}
      onRetryAuto={vi.fn()}
    />,
  );
  return { onRestore, flash, unmount: view.unmount };
}
function chooseFile(value: string, size = value.length, replace = false) {
  fireEvent.change(
    screen.getByLabelText(replace ? "选择文件并覆盖导入" : "导入到空账号"),
    {
      target: { files: [{ size, text: () => Promise.resolve(value) }] },
    },
  );
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

async function acceptImport(replace = false) {
  await screen.findByRole("dialog");
  if (replace) {
    const confirm = screen.getByRole("button", {
      name: "确认覆盖导入",
    }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    expect(accountApi.prepareDataReplacement).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("checkbox", { name: "我已导出当前数据" }));
  }
  fireEvent.click(
    screen.getByRole("button", { name: replace ? "确认覆盖导入" : "确认导入" }),
  );
}

it("cancels an import without writing or refreshing and allows another selection", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(false);
  const { onRestore, flash } = mount();
  chooseFile("{}");
  await screen.findByRole("dialog");
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
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
  await acceptImport();
  await waitFor(() => expect(flash).toHaveBeenCalledWith("数据已导入"));
  expect(accountApi.importData).toHaveBeenCalledWith({ profile: {} });
  expect(onRestore).toHaveBeenCalledOnce();
});

it("downloads the original data before replacing and refreshes after success", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  const replace = vi.fn().mockResolvedValue(undefined);
  const original = { tables: ["original"] };
  vi.mocked(accountApi.prepareDataReplacement).mockResolvedValueOnce({
    data: original,
    replace,
  });
  const { onRestore, flash } = mount();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "覆盖当前账号已有数据" }),
  );
  chooseFile('{"tables":["replacement"]}', undefined, true);
  await acceptImport(true);
  await waitFor(() =>
    expect(flash).toHaveBeenCalledWith("数据已覆盖导入，覆盖前备份已下载"),
  );
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(saveTextDownload).toHaveBeenCalledWith(
    expect.stringMatching(/^mario-before-import-.*\.json$/),
    JSON.stringify(original),
    "application/json",
  );
  expect(replace).toHaveBeenCalledWith({ tables: ["replacement"] });
  expect(vi.mocked(saveTextDownload).mock.invocationCallOrder[0]).toBeLessThan(
    replace.mock.invocationCallOrder[0],
  );
  expect(replace.mock.invocationCallOrder[0]).toBeLessThan(
    onRestore.mock.invocationCallOrder[0],
  );
  expect(accountApi.importData).not.toHaveBeenCalled();
});

it("does not back up or replace when the overwrite confirmation is cancelled", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(false);
  const { onRestore } = mount();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "覆盖当前账号已有数据" }),
  );
  chooseFile("{}", undefined, true);
  await screen.findByRole("dialog");
  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(accountApi.prepareDataReplacement).not.toHaveBeenCalled();
  expect(saveTextDownload).not.toHaveBeenCalled();
  expect(onRestore).not.toHaveBeenCalled();
});

it("keeps existing data when backup export fails", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(accountApi.prepareDataReplacement).mockRejectedValueOnce(
    new Error("备份失败"),
  );
  const { onRestore, flash } = mount();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "覆盖当前账号已有数据" }),
  );
  chooseFile("{}", undefined, true);
  await acceptImport(true);
  expect((await screen.findByRole("alert")).textContent).toContain("备份失败");
  expect(saveTextDownload).not.toHaveBeenCalled();
  expect(accountApi.importData).not.toHaveBeenCalled();
  expect(onRestore).not.toHaveBeenCalled();
  expect(flash).not.toHaveBeenCalled();
  expect(
    (screen.getByLabelText("选择文件并覆盖导入") as HTMLInputElement).disabled,
  ).toBe(false);
});

it("does not replace data when downloading the backup fails", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  const replace = vi.fn();
  vi.mocked(accountApi.prepareDataReplacement).mockResolvedValueOnce({
    data: {},
    replace,
  });
  vi.mocked(saveTextDownload).mockRejectedValueOnce(new Error("下载失败"));
  mount();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "覆盖当前账号已有数据" }),
  );
  chooseFile("{}", undefined, true);
  await acceptImport(true);
  expect((await screen.findByRole("alert")).textContent).toContain("下载失败");
  expect(replace).not.toHaveBeenCalled();
});

it("keeps the backup and reports a rejected replacement without refreshing", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  const replace = vi.fn().mockRejectedValue(new Error("资料已变化"));
  vi.mocked(accountApi.prepareDataReplacement).mockResolvedValueOnce({
    data: {},
    replace,
  });
  const { onRestore, flash } = mount();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "覆盖当前账号已有数据" }),
  );
  chooseFile("{}", undefined, true);
  await acceptImport(true);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "资料已变化",
  );
  expect(saveTextDownload).toHaveBeenCalledOnce();
  expect(onRestore).not.toHaveBeenCalled();
  expect(flash).not.toHaveBeenCalled();
});

it("abandons replacement when the account page unmounts during backup", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  const replace = vi.fn();
  let finish!: (backup: { data: unknown; replace: typeof replace }) => void;
  vi.mocked(accountApi.prepareDataReplacement).mockReturnValueOnce(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const { unmount, onRestore } = mount();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "覆盖当前账号已有数据" }),
  );
  chooseFile("{}", undefined, true);
  await acceptImport(true);
  await waitFor(() =>
    expect(accountApi.prepareDataReplacement).toHaveBeenCalled(),
  );
  unmount();
  finish({ data: {}, replace });
  await waitFor(() => expect(saveTextDownload).not.toHaveBeenCalled());
  expect(replace).not.toHaveBeenCalled();
  expect(onRestore).not.toHaveBeenCalled();
});
