// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ExternalLink } from "./ExternalLink";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});
it.each([
  "javascript:alert(1)",
  "file:///etc/passwd",
  "intent://other-app",
  "invalid",
])("renders an unsafe source URL as text: %s", (href) => {
  render(<ExternalLink href={href}>来源文字</ExternalLink>);
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.getByText("来源文字")).toBeTruthy();
});
it("keeps Web links as normal new-tab links and preserves the source click handler", () => {
  const onClick = vi.fn((event) => event.stopPropagation());
  render(
    <ExternalLink href="https://example.com" onClick={onClick}>
      原始来源
    </ExternalLink>,
  );
  expect(screen.getByRole("link").getAttribute("target")).toBe("_blank");
  expect(screen.getByRole("link").getAttribute("rel")).toBe("noreferrer");
  fireEvent.click(screen.getByRole("link"));
  expect(onClick).toHaveBeenCalledOnce();
});
