// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AccountSettings, WebAccess } from "./WebAccess";
import { savedWebAccounts, setWebToken, webToken } from "../../lib/webSession";
import { acceptRevision, dataRevision } from "../../lib/service";
import { useState } from "react";
vi.mock("../../lib/service", async (original) => ({
  ...(await original<typeof import("../../lib/service")>()),
  isLocalDev: () => false,
  serviceUrl: () =>
    import.meta.env.VITE_API_URL || `${import.meta.env.BASE_URL}api`,
}));
afterEach(() => {
  cleanup();
  sessionStorage.clear();
  localStorage.clear();
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});
it("rejects a remote cleartext API before sending login credentials", async () => {
  vi.stubEnv("VITE_API_URL", "http://remote.example.test/api");
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  render(
    <WebAccess>
      <p>个人资产</p>
    </WebAccess>,
  );
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "user@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  expect((await screen.findByRole("alert")).textContent).toContain("HTTPS");
  expect(fetcher).not.toHaveBeenCalled();
  expect(screen.queryByLabelText("服务器地址")).toBeNull();
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
      <AccountSettings />
    </WebAccess>,
  );
  expect(screen.queryByLabelText("服务器地址")).toBeNull();
  expect(container.querySelector("img")?.getAttribute("src")).toBe(
    "/mario/mario-mark.svg",
  );
  fireEvent.click(screen.getByRole("button", { name: "注册新账号" }));
  expect(screen.queryByLabelText("邀请码")).toBeNull();
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "user@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "创建账号" }));
  await screen.findByText("个人资产");
  expect(fetcher.mock.calls[0][0]).toBe("/mario/api/auth/register");
  expect(JSON.parse(fetcher.mock.calls[0][1].body)).toEqual({
    email: "user@example.test",
    password: "long-password-1234",
  });
  expect(
    JSON.parse(localStorage.getItem("mario.session:/mario/api")!).token,
  ).toBe("mounted-session");
  expect(sessionStorage.getItem("mario.session:/mario/api")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "退出账号" }));
  await screen.findByRole("button", { name: "登录" });
  expect(fetcher.mock.calls[1][0]).toBe("/mario/api/auth/logout");
  expect(localStorage.getItem("mario.session:/mario/api")).toBeNull();
});

it("switches from settings, clears old account drafts, and mounts the next account afresh", async () => {
  function AccountData() {
    const [account] = useState(webToken);
    const [draft, setDraft] = useState("");
    return (
      <>
        <p>{account}</p>
        <input
          aria-label="账号草稿"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <AccountSettings />
      </>
    );
  }
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  setWebToken("account-b", undefined, { email: "b@example.test" });
  setWebToken("account-a", undefined, { email: "a@example.test" });
  acceptRevision("account-a-revision");
  for (const key of [
    "mario.advisorQuestion",
    "compass.advisorQuestion",
    "mario.dailyAssetRange",
  ])
    sessionStorage.setItem(key, "account-a-data");
  render(
    <WebAccess>
      <AccountData />
    </WebAccess>,
  );
  fireEvent.change(screen.getByLabelText("账号草稿"), {
    target: { value: "旧账号输入" },
  });
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  expect((screen.getByLabelText("账号草稿") as HTMLInputElement).value).toBe(
    "旧账号输入",
  );
  fireEvent.click(
    screen.getByRole("button", { name: "切换到 b@example.test" }),
  );
  expect(screen.queryByText("account-a")).toBeNull();
  expect(webToken()).toBe("account-b");
  expect(dataRevision()).toBeUndefined();
  expect(sessionStorage.length).toBe(0);
  await screen.findByText("account-b");
  expect((screen.getByLabelText("账号草稿") as HTMLInputElement).value).toBe(
    "",
  );
  expect(webToken()).toBe("account-b");
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  fireEvent.click(
    screen.getByRole("button", { name: "切换到 a@example.test" }),
  );
  await screen.findByText("account-a");
  expect(webToken()).toBe("account-a");
  expect(fetcher).not.toHaveBeenCalled();
});

it("blocks repeated logouts and retains the account and drafts if logout fails, then allows retry", async () => {
  let fail!: (response: Response) => void;
  const fetcher = vi
    .fn()
    .mockImplementationOnce(
      () =>
        new Promise<Response>((resolve) => {
          fail = resolve;
        }),
    )
    .mockResolvedValueOnce(new Response('{"ok":true}'));
  vi.stubGlobal("fetch", fetcher);
  setWebToken("account-a");
  sessionStorage.setItem("mario.advisorQuestion", "待处理的问题");
  render(
    <WebAccess>
      <p>个人资产</p>
      <AccountSettings />
    </WebAccess>,
  );
  fireEvent.click(screen.getByRole("button", { name: "退出账号" }));
  const switching = screen.getByRole("button", {
    name: "切换账号",
  }) as HTMLButtonElement;
  expect(switching.disabled).toBe(true);
  expect(
    (screen.getByRole("button", { name: "退出账号" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  fireEvent.click(switching);
  expect(fetcher).toHaveBeenCalledTimes(1);
  fail(new Response('{"error":"offline"}', { status: 503 }));
  await screen.findByRole("alert");
  expect(webToken()).toBe("account-a");
  expect(screen.getByText("个人资产")).toBeTruthy();
  expect(sessionStorage.getItem("mario.advisorQuestion")).toBe("待处理的问题");
  fireEvent.click(screen.getByRole("button", { name: "退出账号" }));
  await screen.findByRole("button", { name: "登录" });
  expect(fetcher).toHaveBeenCalledTimes(2);
});

it("adds an account through login, restores both after reopening, and switches back without another login", async () => {
  function AccountData() {
    const [account] = useState(webToken);
    return (
      <>
        <p>{account}</p>
        <AccountSettings />
      </>
    );
  }
  const expiresAt = Math.floor(Date.now() / 1000) + 3600;
  const fetcher = vi.fn().mockResolvedValueOnce(
    new Response(
      JSON.stringify({
        token: "account-b",
        expiresAt,
        email: "b@example.test",
        userId: "b",
      }),
    ),
  );
  vi.stubGlobal("fetch", fetcher);
  setWebToken("account-a", expiresAt, { email: "a@example.test", userId: "a" });
  const { unmount } = render(
    <WebAccess>
      <AccountData />
    </WebAccess>,
  );
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  fireEvent.click(screen.getByRole("button", { name: "登录其他账号" }));
  expect(webToken()).toBe("account-a");
  expect(screen.queryByText("account-a")).toBeNull();
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "b@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await screen.findByText("account-b");
  expect(fetcher.mock.calls[0][0]).toBe("/api/auth/login");
  expect(savedWebAccounts().map((a) => a.email)).toEqual([
    "a@example.test",
    "b@example.test",
  ]);
  expect(localStorage.getItem("mario.accounts:/api")).not.toContain(
    "long-password-1234",
  );
  unmount();
  sessionStorage.clear();
  render(
    <WebAccess>
      <AccountData />
    </WebAccess>,
  );
  expect(screen.getByText("account-b")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  fireEvent.click(
    screen.getByRole("button", { name: "切换到 a@example.test" }),
  );
  await screen.findByText("account-a");
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("allows cancelling the picker and a failed additional login while preserving the current account", async () => {
  const fetcher = vi
    .fn()
    .mockResolvedValue(
      new Response('{"error":"邮箱或密码错误"}', { status: 401 }),
    );
  vi.stubGlobal("fetch", fetcher);
  setWebToken("account-a", undefined, { email: "a@example.test" });
  render(
    <WebAccess>
      <p>个人资产</p>
      <AccountSettings />
    </WebAccess>,
  );
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  fireEvent.click(screen.getByRole("button", { name: "取消切换" }));
  expect(webToken()).toBe("account-a");
  expect(fetcher).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  fireEvent.click(screen.getByRole("button", { name: "登录其他账号" }));
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "b@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "wrong-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await screen.findByRole("alert");
  expect(webToken()).toBe("account-a");
  fireEvent.click(screen.getByRole("button", { name: "返回当前账号" }));
  await screen.findByText("个人资产");
  expect(savedWebAccounts().map((a) => a.token)).toEqual(["account-a"]);
});

it("removes a revoked selected account while keeping another saved account available", async () => {
  setWebToken("account-a", undefined, { email: "a@example.test" });
  setWebToken("account-b", undefined, { email: "b@example.test" });
  render(
    <WebAccess>
      <p>个人资产</p>
      <AccountSettings />
    </WebAccess>,
  );
  fireEvent(window, new Event("mario:session-expired"));
  await screen.findByRole("button", { name: "登录" });
  expect(
    screen.queryByRole("button", { name: "切换到 b@example.test" }),
  ).toBeNull();
  expect(savedWebAccounts().map((a) => a.token)).toEqual(["account-a"]);
  fireEvent.click(
    screen.getByRole("button", { name: "切换到 a@example.test" }),
  );
  await screen.findByText("个人资产");
  expect(webToken()).toBe("account-a");
});

it("does not overwrite another tab's account switch when an additional login completes late", async () => {
  let complete!: (response: Response) => void;
  vi.stubGlobal(
    "fetch",
    vi.fn().mockImplementation(
      () =>
        new Promise<Response>((resolve) => {
          complete = resolve;
        }),
    ),
  );
  setWebToken("account-a", undefined, { email: "a@example.test" });
  render(
    <WebAccess>
      <p>个人资产</p>
      <AccountSettings />
    </WebAccess>,
  );
  fireEvent.click(screen.getByRole("button", { name: "切换账号" }));
  fireEvent.click(screen.getByRole("button", { name: "登录其他账号" }));
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "c@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码"), {
    target: { value: "long-password-1234" },
  });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  setWebToken("account-b", undefined, { email: "b@example.test" });
  fireEvent(window, new StorageEvent("storage", { storageArea: localStorage }));
  complete(new Response('{"token":"account-c"}'));
  await waitFor(() =>
    expect(
      (screen.getByRole("button", { name: "切换账号" }) as HTMLButtonElement)
        .disabled,
    ).toBe(false),
  );
  expect(webToken()).toBe("account-b");
  expect(savedWebAccounts().map((a) => a.token)).toEqual([
    "account-a",
    "account-b",
  ]);
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
      <AccountSettings />
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
    email: "person@example.test",
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
      <AccountSettings />
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
