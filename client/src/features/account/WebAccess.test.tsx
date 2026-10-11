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
import { setWebToken, webToken } from "../../lib/webSession";
import { useState } from "react";
vi.mock("../../lib/service", async (original) => ({
  ...(await original<typeof import("../../lib/service")>()),
  isLocalDev: () => false,
  serviceUrl: () => `${import.meta.env.BASE_URL}api`,
}));
afterEach(() => {
  cleanup();
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});
it("registers and logs out through the mounted API and loads the mounted logo", async () => {
  vi.stubEnv("BASE_URL", "/mario/");
  const fetcher = vi
    .fn()
    .mockResolvedValueOnce(new Response('{"token":"mounted-session"}'))
    .mockResolvedValueOnce(new Response('{"ok":true}'));
  vi.stubGlobal("fetch", fetcher);
  const { container } = render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  expect(screen.queryByLabelText("服务器地址")).toBeNull();
  expect(container.querySelector("img")?.getAttribute("src")).toBe(
    "/mario/mario-mark.svg",
  );
  fireEvent.click(screen.getByRole("button", { name: "使用邀请码注册" }));
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "user@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.change(screen.getByLabelText("邀请码"), {
    target: { value: "invite" },
  });
  fireEvent.click(screen.getByRole("button", { name: "创建账号" }));
  await screen.findByText("个人资产");
  expect(fetcher.mock.calls[0][0]).toBe("/mario/api/auth/register");
  expect(
    JSON.parse(localStorage.getItem("mario.session:/mario/api")!).token,
  ).toBe("mounted-session");
  expect(sessionStorage.getItem("mario.session:/mario/api")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "退出账号" }));
  await screen.findByRole("button", { name: "创建账号" });
  expect(fetcher.mock.calls[1][0]).toBe("/mario/api/auth/logout");
  expect(localStorage.getItem("mario.session:/mario/api")).toBeNull();
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
  expect(webToken()).toBeUndefined();
  expect(localStorage.getItem("mario.session:/api")).toBeNull();
  expect(sessionStorage.getItem("mario.session:/api")).toBeNull();
});

it("restores a login after remounting and reopening without logging in again", async () => {
  const expiresAt = Math.floor(Date.now() / 1000) + 3600;
  const fetcher = vi
    .fn()
    .mockResolvedValue(
      new Response(JSON.stringify({ token: "saved-session", expiresAt })),
    );
  vi.stubGlobal("fetch", fetcher);
  const { unmount } = render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "person@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await screen.findByText("个人资产");
  expect(JSON.parse(localStorage.getItem("mario.session:/api")!)).toEqual({
    token: "saved-session",
    expiresAt,
  });
  unmount();
  sessionStorage.clear();
  render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  expect(screen.getByText("个人资产")).toBeTruthy();
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("resets account data when another tab changes accounts or logs out", () => {
  function AccountData() {
    const [account] = useState(webToken);
    return <p>{account}</p>;
  }
  setWebToken("account-a");
  render(
    <WebAccess>
      <AccountData />
    </WebAccess>,
  );
  expect(screen.getByText("account-a")).toBeTruthy();
  setWebToken("account-b");
  fireEvent(
    window,
    new StorageEvent("storage", {
      key: "mario.session:/api",
      storageArea: localStorage,
    }),
  );
  expect(screen.queryByText("account-a")).toBeNull();
  expect(screen.getByText("account-b")).toBeTruthy();
  localStorage.clear();
  fireEvent(window, new StorageEvent("storage", { storageArea: localStorage }));
  expect(screen.queryByText("account-b")).toBeNull();
  expect(screen.getByRole("button", { name: "登录" })).toBeTruthy();
});

it("does not clear a new login when an earlier logout finishes", async () => {
  let complete!: (response: Response) => void;
  const fetcher = vi.fn().mockImplementation(
    () =>
      new Promise<Response>((resolve) => {
        complete = resolve;
      }),
  );
  vi.stubGlobal("fetch", fetcher);
  setWebToken("old-session");
  render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  fireEvent.click(screen.getByRole("button", { name: "退出账号" }));
  setWebToken("new-session");
  fireEvent(window, new StorageEvent("storage", { storageArea: localStorage }));
  complete(new Response('{"ok":true}'));
  await waitFor(() => expect(webToken()).toBe("new-session"));
  expect(screen.getByText("个人资产")).toBeTruthy();
  expect(fetcher.mock.calls[0][1].headers.Authorization).toBe(
    "Bearer old-session",
  );
});
