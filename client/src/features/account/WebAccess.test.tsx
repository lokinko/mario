// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WebAccess } from "./WebAccess";
import { webToken } from "../../lib/webSession";
vi.mock("../../lib/webSession", async (original) => ({
  ...(await original<typeof import("../../lib/webSession")>()),
  isWebApp: () => true,
}));
afterEach(() => {
  cleanup();
  sessionStorage.clear();
  vi.unstubAllGlobals();
});
it("keeps assets hidden until the server accepts the key and clears the session on disconnect", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(new Response("unauthorized", { status: 401 }))
    .mockResolvedValueOnce(new Response('{"status":"ok"}'));
  vi.stubGlobal("fetch", fetcher);
  render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  expect(screen.queryByText("个人资产")).toBeNull();
  fireEvent.change(screen.getByLabelText("访问密钥"), {
    target: { value: "wrong" },
  });
  fireEvent.click(screen.getByRole("button", { name: "连接" }));
  await screen.findByRole("alert");
  expect(webToken()).toBeUndefined();
  fireEvent.change(screen.getByLabelText("访问密钥"), {
    target: { value: "valid-key" },
  });
  fireEvent.click(screen.getByRole("button", { name: "连接" }));
  await screen.findByText("个人资产");
  expect(fetcher.mock.calls[1][1].headers.Authorization).toBe(
    "Bearer valid-key",
  );
  expect(webToken()).toBe("valid-key");
  fireEvent.click(screen.getByRole("button", { name: "断开 Web 连接" }));
  expect(screen.queryByText("个人资产")).toBeNull();
  expect(webToken()).toBeUndefined();
});
