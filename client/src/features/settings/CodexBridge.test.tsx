// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { CodexBridge } from "./CodexBridge";
import * as api from "../../api/settings";
import type { ModelConfig } from "../../types";
vi.mock("../../api/settings", () => ({
  getCodexBridgeStatus: vi.fn(),
  pairCodexBridge: vi.fn(),
  enableCodexBridge: vi.fn(),
  revokeCodexBridge: vi.fn(),
  getModelConfig: vi.fn(),
}));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
it("explains both local execution choices without asking the webpage for ChatGPT credentials", async () => {
  vi.mocked(api.getCodexBridgeStatus).mockResolvedValue({
    paired: false,
    online: false,
    model: null,
  });
  render(<CodexBridge onUpdate={vi.fn()} disabled={false} />);
  await screen.findByText("尚未配对或配对已过期");
  expect(screen.getByText("npm run chatgpt:bridge")).toBeTruthy();
  expect(screen.getByText("npm run codex:bridge")).toBeTruthy();
  expect(screen.getByText(/没有已选择账号时会打开浏览器授权/)).toBeTruthy();
  expect(screen.getByText(/浏览器回调仅监听 127\.0\.0\.1/)).toBeTruthy();
  expect(screen.getByText(/不代表公开或商业化托管服务已获准使用/)).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  expect(screen.queryByLabelText(/密码|令牌|API Key/)).toBeNull();
});
it("requires an online paired computer before enabling and keeps pairing code out of storage", async () => {
  vi.mocked(api.getCodexBridgeStatus).mockResolvedValue({
    paired: false,
    online: false,
    model: null,
  });
  vi.mocked(api.pairCodexBridge).mockResolvedValue({
    pairingCode: "a".repeat(64),
    expiresInSeconds: 600,
  });
  const storage = vi.spyOn(Storage.prototype, "setItem");
  render(<CodexBridge onUpdate={vi.fn()} disabled={false} />);
  await screen.findByText("尚未配对或配对已过期");
  expect((screen.getByText("启用本机模型") as HTMLButtonElement).disabled).toBe(
    true,
  );
  fireEvent.click(screen.getByText("生成配对码"));
  const code = await screen.findByLabelText("配对码");
  expect((code as HTMLInputElement).type).toBe("password");
  expect((code as HTMLInputElement).value).toBe("a".repeat(64));
  expect(storage).not.toHaveBeenCalled();
  storage.mockRestore();
});
it("enables the detected model and revokes the account pairing", async () => {
  const model = {
    provider: "codex-bridge" as const,
    baseUrl: "codex://bridge",
    model: "fixture",
    hasApiKey: true,
  };
  vi.mocked(api.getCodexBridgeStatus).mockResolvedValue({
    paired: true,
    online: true,
    model: "fixture",
  });
  vi.mocked(api.enableCodexBridge).mockResolvedValue(model);
  vi.mocked(api.revokeCodexBridge).mockResolvedValue({ ok: true });
  vi.mocked(api.getModelConfig).mockResolvedValue({
    ...model,
    hasApiKey: false,
  });
  const update = vi.fn();
  render(<CodexBridge onUpdate={update} disabled={false} />);
  await screen.findByText("本机在线 · 默认模型 fixture");
  fireEvent.click(screen.getByText("启用本机模型"));
  await waitFor(() => expect(update).toHaveBeenCalledWith(model));
  fireEvent.click(screen.getByText("撤销配对"));
  await waitFor(() =>
    expect(update).toHaveBeenCalledWith({ ...model, hasApiKey: false }),
  );
  expect(screen.queryByLabelText("配对码")).toBeNull();
});
it("does not update the account after the component unmounts during enable", async () => {
  vi.mocked(api.getCodexBridgeStatus).mockResolvedValue({
    paired: true,
    online: true,
    model: "fixture",
  });
  let resolve!: (model: ModelConfig) => void;
  vi.mocked(api.enableCodexBridge).mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const update = vi.fn();
  const view = render(<CodexBridge onUpdate={update} disabled={false} />);
  await screen.findByText("本机在线 · 默认模型 fixture");
  fireEvent.click(screen.getByText("启用本机模型"));
  view.unmount();
  resolve({
    provider: "codex-bridge",
    baseUrl: "codex://bridge",
    model: "fixture",
    hasApiKey: true,
  });
  await Promise.resolve();
  expect(update).not.toHaveBeenCalled();
});
