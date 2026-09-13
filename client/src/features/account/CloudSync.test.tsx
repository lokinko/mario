// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { CloudSync } from "./CloudSync";
import * as api from "../../api";
import { defaultCloudConfig } from "../../lib/cloudConfig";

vi.mock("../../api", () => ({
  getCloudConfig: vi.fn(),
  getCloudStatus: vi.fn(),
  saveCloudConfig: vi.fn(),
  pullCloudSync: vi.fn(),
  pushCloudSync: vi.fn(),
  exportCloudRecoveryKey: vi.fn(),
  importCloudRecoveryKey: vi.fn(),
  resendCloudConfirmation: vi.fn(),
  requestCloudPasswordReset: vi.fn(),
  resetCloudPassword: vi.fn(),
  signInCloud: vi.fn(),
  signOutCloud: vi.fn(),
  signUpCloud: vi.fn(),
  verifyCloudPasswordReset: vi.fn(),
}));
const status = {
  configured: true,
  signedIn: false,
  emailConfirmationPending: false,
  hasRecoveryKey: false,
  baseRevision: 0,
  localChangedSinceSync: false,
  privacyBoundary: [],
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(api.getCloudConfig).mockResolvedValue({
    url: "https://example.supabase.co",
    publishableKey: "public",
  });
  vi.mocked(api.getCloudStatus).mockResolvedValue(status);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it("shows initialization errors and allows a successful retry", async () => {
  vi.mocked(api.getCloudConfig).mockRejectedValueOnce(new Error("连接失败"));
  render(<CloudSync flash={vi.fn()} onRestore={vi.fn()} />);
  expect((await screen.findByRole("alert")).textContent).toContain("连接失败");
  expect(screen.queryByText("正在读取账户与同步状态…")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "重新尝试" }));
  expect(await screen.findByRole("heading", { name: "登录账户" })).toBeTruthy();
  const advanced = screen.getByText("自定义云端服务").closest("details");
  expect(advanced?.open).toBe(false);
  expect(api.saveCloudConfig).not.toHaveBeenCalled();
});

it("configures a fresh installation and enables login after credentials are entered", async () => {
  vi.mocked(api.getCloudConfig).mockResolvedValue(null);
  vi.mocked(api.saveCloudConfig).mockResolvedValue(status);
  vi.mocked(api.signInCloud).mockResolvedValue({
    signedIn: true,
    email: "test@example.com",
    emailConfirmationPending: false,
    message: "已登录",
  });
  render(<CloudSync flash={vi.fn()} onRestore={vi.fn()} />);
  const login = await screen.findByRole("button", { name: "登录" });
  expect(api.saveCloudConfig).toHaveBeenCalledWith(defaultCloudConfig);
  expect(screen.getByText("自定义云端服务").closest("details")?.open).toBe(
    false,
  );
  expect((login as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText("请输入邮箱和密码后登录。")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "test@example.com" },
  });
  // Existing accounts may have passwords shorter than the signup minimum.
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "oldpwd" },
  });
  expect((login as HTMLButtonElement).disabled).toBe(false);
  fireEvent.click(login);
  await waitFor(() =>
    expect(api.signInCloud).toHaveBeenCalledWith("test@example.com", "oldpwd"),
  );
});

it("explains why login is unavailable when the account service is unconfigured", async () => {
  vi.mocked(api.getCloudStatus).mockResolvedValue({
    ...status,
    configured: false,
  });
  render(<CloudSync flash={vi.fn()} onRestore={vi.fn()} />);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "尚未连接账户服务",
  );
  expect(
    (screen.getByRole("button", { name: "登录" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});

it("shows a retry action if saving the default configuration fails", async () => {
  vi.mocked(api.getCloudConfig).mockResolvedValue(null);
  vi.mocked(api.saveCloudConfig).mockRejectedValueOnce(
    new Error("配置保存失败"),
  );
  render(<CloudSync flash={vi.fn()} onRestore={vi.fn()} />);
  expect((await screen.findByRole("alert")).textContent).toContain(
    "配置保存失败",
  );
  vi.mocked(api.saveCloudConfig).mockResolvedValue(status);
  fireEvent.click(screen.getByRole("button", { name: "重新尝试" }));
  expect(await screen.findByRole("button", { name: "登录" })).toBeTruthy();
});

it("ignores initialization results arriving after unmount", async () => {
  let resolve!: (value: null) => void;
  vi.mocked(api.getCloudConfig).mockReturnValue(
    new Promise((r) => {
      resolve = r;
    }),
  );
  const view = render(<CloudSync flash={vi.fn()} onRestore={vi.fn()} />);
  view.unmount();
  resolve(null);
  await waitFor(() => expect(api.getCloudConfig).toHaveBeenCalledTimes(1));
  expect(api.getCloudStatus).not.toHaveBeenCalled();
});

it("refreshes investment data after an explicitly confirmed cloud restore", async () => {
  vi.mocked(api.getCloudStatus).mockResolvedValue({
    ...status,
    signedIn: true,
    email: "test@example.com",
    hasRecoveryKey: true,
  });
  vi.mocked(api.pullCloudSync).mockResolvedValue({
    direction: "pull",
    revision: 2,
    contentHash: "test",
    recordCount: 3,
    syncedAt: "2026-09-09",
    message: "恢复完成",
  });
  vi.spyOn(window, "confirm").mockReturnValue(false);
  const onRestore = vi.fn().mockResolvedValue(undefined);
  render(<CloudSync flash={vi.fn()} onRestore={onRestore} />);
  const restore = await screen.findByRole("button", {
    name: "拉取并替换本机数据",
  });
  fireEvent.click(restore);
  expect(api.pullCloudSync).not.toHaveBeenCalled();
  vi.mocked(window.confirm).mockReturnValue(true);
  fireEvent.click(restore);
  await waitFor(() => expect(onRestore).toHaveBeenCalledTimes(1));
  expect(api.pullCloudSync).toHaveBeenCalledWith(true);
  expect(await screen.findByText(/恢复完成/)).toBeTruthy();
});

it("merges by default without asking to replace local data", async () => {
  vi.mocked(api.getCloudStatus).mockResolvedValue({
    ...status,
    signedIn: true,
    hasRecoveryKey: true,
  });
  vi.mocked(api.pullCloudSync).mockResolvedValue({
    direction: "pull",
    revision: 2,
    contentHash: "test",
    recordCount: 3,
    syncedAt: "2026-09-11",
    message: "合并完成",
  });
  const confirm = vi.spyOn(window, "confirm");
  const onRestore = vi.fn().mockResolvedValue(undefined);
  render(<CloudSync flash={vi.fn()} onRestore={onRestore} />);
  fireEvent.click(await screen.findByRole("button", { name: "拉取并合并" }));
  await waitFor(() => expect(onRestore).toHaveBeenCalled());
  expect(api.pullCloudSync).toHaveBeenCalledWith(false);
  expect(confirm).not.toHaveBeenCalled();
});
