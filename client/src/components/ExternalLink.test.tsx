// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink } from "./ExternalLink";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});
function native() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: {},
    configurable: true,
  });
}
it("opens the system browser even when the parent source detail stops propagation", async () => {
  native();
  vi.mocked(openUrl).mockResolvedValue(undefined);
  render(
    <ExternalLink
      href="https://example.com/report"
      onClick={(event) => event.stopPropagation()}
    >
      原始来源
    </ExternalLink>,
  );
  fireEvent.click(screen.getByRole("link"));
  await waitFor(() =>
    expect(openUrl).toHaveBeenCalledWith("https://example.com/report"),
  );
});
it.each([
  "javascript:alert(1)",
  "file:///etc/passwd",
  "intent://other-app",
  "invalid",
])("renders an unsafe source URL as text: %s", (href) => {
  native();
  render(<ExternalLink href={href}>来源文字</ExternalLink>);
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.getByText("来源文字")).toBeTruthy();
  expect(openUrl).not.toHaveBeenCalled();
});
it("shows a native browser error and preserves the source URL", async () => {
  native();
  vi.mocked(openUrl).mockRejectedValue(new Error("无可用浏览器"));
  render(<ExternalLink href="https://example.com">原始来源</ExternalLink>);
  fireEvent.click(screen.getByRole("link"));
  expect((await screen.findByRole("alert")).textContent).toContain(
    "无可用浏览器",
  );
  expect(screen.getByRole("link").getAttribute("href")).toBe(
    "https://example.com/",
  );
});
it("keeps Web links as normal new-tab links", () => {
  render(<ExternalLink href="https://example.com">原始来源</ExternalLink>);
  expect(screen.getByRole("link").getAttribute("target")).toBe("_blank");
  expect(screen.getByRole("link").getAttribute("rel")).toBe("noreferrer");
  expect(openUrl).not.toHaveBeenCalled();
});
