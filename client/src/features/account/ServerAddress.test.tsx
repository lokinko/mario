// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import * as accountApi from "../../api/account";
import {
  defaultServiceUrl,
  serviceUrl,
  serviceUrlStorageKey,
} from "../../lib/service";
import { ServerAddress } from "./ServerAddress";

vi.mock("../../api/account", () => ({ probeService: vi.fn() }));
afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

function edit(value: string) {
  fireEvent.click(screen.getByRole("button", { name: "修改" }));
  fireEvent.change(screen.getByLabelText("服务器地址"), {
    target: { value },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存并重新连接" }));
}

it("switches only after the new server answers", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(accountApi.probeService).mockResolvedValue(undefined);
  const onSwitched = vi.fn();
  render(<ServerAddress onSwitched={onSwitched} />);
  edit("https://example.com/api/");
  await waitFor(() => expect(onSwitched).toHaveBeenCalledOnce());
  expect(accountApi.probeService).toHaveBeenCalledWith(
    "https://example.com/api",
  );
  expect(serviceUrl()).toBe("https://example.com/api");
});

it("keeps the current server when the new one is unreachable or invalid", async () => {
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(accountApi.probeService).mockRejectedValue(
    new Error("无法连接 https://down.example/api"),
  );
  const onSwitched = vi.fn();
  render(<ServerAddress onSwitched={onSwitched} />);
  edit("https://down.example/api");
  expect((await screen.findByRole("alert")).textContent).toContain("无法连接");
  expect(serviceUrl()).toBe(defaultServiceUrl());
  fireEvent.change(screen.getByLabelText("服务器地址"), {
    target: { value: "ftp://example.com" },
  });
  fireEvent.click(screen.getByRole("button", { name: "保存并重新连接" }));
  expect((await screen.findByRole("alert")).textContent).toContain("http://");
  expect(onSwitched).not.toHaveBeenCalled();
});

it("offers a reset once a custom server is saved", async () => {
  localStorage.setItem(serviceUrlStorageKey(), "https://example.com/api");
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.mocked(accountApi.probeService).mockResolvedValue(undefined);
  const onSwitched = vi.fn();
  render(<ServerAddress onSwitched={onSwitched} />);
  expect(screen.getByText("https://example.com/api")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "恢复默认" }));
  await waitFor(() => expect(onSwitched).toHaveBeenCalledOnce());
  expect(serviceUrl()).toBe(defaultServiceUrl());
});
