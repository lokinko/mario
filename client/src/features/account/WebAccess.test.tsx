// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WebAccess } from "./WebAccess";
import { webToken } from "../../lib/webSession";
vi.mock("../../lib/service", async (original) => ({
  ...(await original<typeof import("../../lib/service")>()),
  isLocalDev: () => false,
  serviceUrl: () => "/api",
}));
afterEach(() => {
  cleanup();
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
});
it("keeps data hidden until login, and revokes the session before disconnecting", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(
      new Response('{"error":"邮箱或密码错误"}', { status: 401 }),
    )
    .mockResolvedValueOnce(new Response('{"token":"valid-session"}'))
    .mockResolvedValueOnce(new Response('{"ok":true}'));
  vi.stubGlobal("fetch", fetcher);
  render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  expect(screen.queryByText("个人资产")).toBeNull();
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "person@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await screen.findByRole("alert");
  expect(webToken()).toBeUndefined();
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await screen.findByText("个人资产");
  expect(webToken()).toBe("valid-session");
  fireEvent.click(screen.getByRole("button", { name: "退出账号" }));
  await waitFor(() => expect(screen.queryByText("个人资产")).toBeNull());
  expect(fetcher.mock.calls[2][1].headers.Authorization).toBe(
    "Bearer valid-session",
  );
  expect(webToken()).toBeUndefined();
});
it("a revoked or expired session unmounts account data", async () => {
  sessionStorage.setItem("mario.session:/api", "expired");
  render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  fireEvent(window, new Event("mario:session-expired"));
  await screen.findByRole("button", { name: "登录" });
  expect(screen.queryByText("个人资产")).toBeNull();
});
