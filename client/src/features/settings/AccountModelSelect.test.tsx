// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { AccountModelSelect } from "./AccountModelSelect";
import * as api from "../../api/settings";

vi.mock("../../api/settings", () => ({ fetchAccountModels: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

it("auto-fetches available models for a saved binding and lets the user pick one", async () => {
  vi.mocked(api.fetchAccountModels).mockResolvedValue({
    models: ["gpt-5.4", "gpt-5.4-mini"],
  });
  const onChange = vi.fn();
  render(
    <AccountModelSelect
      provider="openai-responses"
      baseUrl="https://api.openai.com/v1"
      apiKey=""
      value="gpt-4.1-mini"
      savedModel="gpt-4.1-mini"
      hasSavedBinding
      disabled={false}
      onChange={onChange}
    />,
  );
  await screen.findByRole("option", { name: "gpt-5.4" });
  expect(api.fetchAccountModels).toHaveBeenCalledWith({
    provider: "openai-responses",
    baseUrl: "https://api.openai.com/v1",
    apiKey: undefined,
  });
  expect(
    screen.getByRole("option", { name: "gpt-4.1-mini · 已保存" }),
  ).toBeTruthy();
  fireEvent.change(screen.getByLabelText("模型名称"), {
    target: { value: "gpt-5.4" },
  });
  expect(onChange).toHaveBeenCalledWith("gpt-5.4");
});

it("fetches models using a newly typed key and resets old models when endpoint changes", async () => {
  vi.mocked(api.fetchAccountModels).mockResolvedValue({
    models: ["claude-sonnet-4-5", "claude-haiku-4-5"],
  });
  const onChange = vi.fn();
  const view = render(
    <AccountModelSelect
      provider="anthropic"
      baseUrl="https://api.anthropic.com/v1"
      apiKey="draft-key"
      value=""
      savedModel=""
      hasSavedBinding={false}
      disabled={false}
      onChange={onChange}
    />,
  );
  expect(api.fetchAccountModels).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "获取可用模型" }));
  await screen.findByRole("option", { name: "claude-sonnet-4-5" });
  expect(api.fetchAccountModels).toHaveBeenCalledWith({
    provider: "anthropic",
    baseUrl: "https://api.anthropic.com/v1",
    apiKey: "draft-key",
  });
  view.rerender(
    <AccountModelSelect
      provider="openai-chat"
      baseUrl="https://gateway.example.test/v1"
      apiKey=""
      value=""
      savedModel=""
      hasSavedBinding={false}
      disabled={false}
      onChange={onChange}
    />,
  );
  expect(
    screen.queryByRole("option", { name: "claude-sonnet-4-5" }),
  ).toBeNull();
  expect(
    (screen.getByRole("button", { name: "获取可用模型" }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
});

it("allows adding a custom model name and rejects whitespace inside model ids", async () => {
  vi.mocked(api.fetchAccountModels).mockRejectedValue(
    new Error("模型目录返回 HTTP 404，可直接添加自定义模型名"),
  );
  let selected = "";
  const onChange = vi.fn((next: string) => {
    selected = next;
  });
  const view = render(
    <AccountModelSelect
      provider="openai-chat"
      baseUrl="https://gateway.example.test/v1"
      apiKey="custom-key"
      value={selected}
      savedModel=""
      hasSavedBinding={false}
      disabled={false}
      onChange={onChange}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "获取可用模型" }));
  await screen.findByText(/模型目录返回 HTTP 404/);

  fireEvent.click(screen.getByRole("button", { name: "添加自定义模型" }));
  fireEvent.change(screen.getByLabelText("自定义模型名称"), {
    target: { value: "gpt-6.2 sol" },
  });
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  expect(screen.getByText("模型名称不能包含空格")).toBeTruthy();
  expect(onChange).not.toHaveBeenCalled();

  fireEvent.change(screen.getByLabelText("自定义模型名称"), {
    target: { value: "  deepseek-chat  " },
  });
  fireEvent.click(screen.getByRole("button", { name: "添加" }));
  expect(onChange).toHaveBeenCalledWith("deepseek-chat");

  view.rerender(
    <AccountModelSelect
      provider="openai-chat"
      baseUrl="https://gateway.example.test/v1"
      apiKey="custom-key"
      value="deepseek-chat"
      savedModel=""
      hasSavedBinding={false}
      disabled={false}
      onChange={onChange}
    />,
  );
  await waitFor(() =>
    expect(
      screen.getByRole("option", { name: "deepseek-chat · 自定义" }),
    ).toBeTruthy(),
  );
  expect((screen.getByLabelText("模型名称") as HTMLSelectElement).value).toBe(
    "deepseek-chat",
  );
});
