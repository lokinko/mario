// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import App from "./App";
import * as api from "./api";

vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  getSnapshot: vi.fn(),
  getModelConfig: vi.fn(),
}));
vi.mock("./reminders", () => ({
  checkAndSendReviewReminder: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("./features/account/CloudSync", () => ({
  CloudSync: ({ onRestore }: { onRestore: () => Promise<void> }) => (
    <button onClick={() => void onRestore()}>测试恢复数据</button>
  ),
}));
beforeEach(() => {
  vi.clearAllMocks();
  window.location.hash = "cloud";
});
afterEach(cleanup);

it("keeps local application accessible when optional model configuration fails", async () => {
  vi.mocked(api.getSnapshot).mockResolvedValue(
    {} as Awaited<ReturnType<typeof api.getSnapshot>>,
  );
  vi.mocked(api.getModelConfig).mockRejectedValue(
    new Error("keychain unavailable"),
  );
  render(<App />);
  expect(
    await screen.findByRole("button", { name: "测试恢复数据" }),
  ).toBeTruthy();
  expect((await screen.findByRole("alert")).textContent).toContain(
    "本地决策与复盘不受影响",
  );
  expect(screen.queryByText("本地服务尚未就绪")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "测试恢复数据" }));
  await waitFor(() => expect(api.getSnapshot).toHaveBeenCalledTimes(2));
});

it("offers retry when required local data fails to load", async () => {
  vi.mocked(api.getSnapshot)
    .mockRejectedValueOnce(new Error("local unavailable"))
    .mockResolvedValue({} as Awaited<ReturnType<typeof api.getSnapshot>>);
  vi.mocked(api.getModelConfig).mockRejectedValue(
    new Error("model unavailable"),
  );
  render(<App />);
  fireEvent.click(await screen.findByRole("button", { name: "重新连接" }));
  expect(
    await screen.findByRole("button", { name: "测试恢复数据" }),
  ).toBeTruthy();
});
