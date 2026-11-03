// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { OpenAiSignIn } from "./OpenAiSignIn";
import { requestJson } from "../../lib/transport";
import { openAiSignIn } from "./openaiLogin";
vi.mock("../../lib/transport", () => ({ requestJson: vi.fn() }));
vi.mock("../../lib/service", () => ({ serviceUrl: () => "/api" }));
vi.mock("./openaiLogin", () => ({ openAiSignIn: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});
it("hides unconfigured website sign-in", async () => {
  vi.mocked(requestJson).mockResolvedValue({ enabled: false });
  render(<OpenAiSignIn />);
  await Promise.resolve();
  expect(screen.queryByRole("button")).toBeNull();
});
it("provides a recognizable login button and reports authorization-start failures", async () => {
  vi.mocked(requestJson).mockResolvedValue({ enabled: true });
  vi.mocked(openAiSignIn).mockRejectedValue(new Error("服务暂不可用"));
  render(<OpenAiSignIn />);
  const button = await screen.findByRole("button", {
    name: "Continue with ChatGPT",
  });
  expect(button.hasAttribute("data-oai-siwc-button")).toBe(true);
  fireEvent.click(button);
  await Promise.resolve();
  expect(await screen.findByText("服务暂不可用")).toBeTruthy();
});
