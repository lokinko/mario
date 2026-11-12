// @vitest-environment jsdom
import { useState } from "react";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { AmountPrivacy, Money, PrivateNumberInput } from "./AmountPrivacy";
import { useDisplayPreferences } from "../app/useDisplayPreferences";

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-mask-amounts");
  delete document.documentElement.dataset.pnl;
});

function Portfolio() {
  const display = useDisplayPreferences();
  const [amount, setAmount] = useState("1234.5678");
  return (
    <AmountPrivacy.Provider value={display.maskAmounts}>
      <button onClick={() => display.setMaskAmounts(!display.maskAmounts)}>
        切换金额显示
      </button>
      <table>
        <tbody>
          <tr>
            <th>市值</th>
            <Money as="td">{amount}</Money>
            <td>数量 12 · 占比 3%</td>
          </tr>
        </tbody>
      </table>
      <label>
        当前金额
        <PrivateNumberInput
          data-money-input
          value={amount}
          onChange={(event) => setAmount(event.target.value)}
          min={0}
          max={10000}
          step="any"
          required
        />
      </label>
      <label>
        数量
        <PrivateNumberInput value="12" readOnly />
      </label>
    </AmountPrivacy.Provider>
  );
}

it("hides the accessible amount while preserving quantities, labels and live updates", () => {
  render(<Portfolio />);
  expect(screen.getByRole("row", { name: /1234\.5678/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "切换金额显示" }));
  expect(screen.queryByRole("row", { name: /1234\.5678/ })).toBeNull();
  expect(
    screen.getByRole("row", { name: "市值 金额已隐藏 数量 12 · 占比 3%" }),
  ).toBeTruthy();
  const input = screen.getByLabelText<HTMLInputElement>("当前金额");
  expect(input.type).toBe("password");
  expect(screen.getByLabelText<HTMLInputElement>("数量").type).toBe("number");
  fireEvent.change(input, { target: { value: "9876.54321" } });
  expect(screen.queryByRole("row", { name: /9876\.54321/ })).toBeNull();
  expect(input.value).toBe("9876.54321");
  fireEvent.click(screen.getByRole("button", { name: "切换金额显示" }));
  expect(input.type).toBe("number");
  expect(input.value).toBe("9876.54321");
  expect(screen.getByRole("row", { name: /9876\.54321/ })).toBeTruthy();
  expect(screen.queryByText("金额已隐藏")).toBeNull();
});

it("protects newly mounted amounts when the saved preference is hidden", () => {
  localStorage.setItem("mario.display", JSON.stringify({ maskAmounts: true }));
  const view = render(<Portfolio />);
  expect(screen.queryByRole("row", { name: /1234\.5678/ })).toBeNull();
  expect(screen.getByLabelText<HTMLInputElement>("当前金额").type).toBe(
    "password",
  );
  act(() => view.unmount());
  render(<Portfolio />);
  expect(screen.getByRole("cell", { name: "金额已隐藏" })).toBeTruthy();
});

it("keeps native numeric validation in masked inputs and restores it on reveal", () => {
  render(<Portfolio />);
  fireEvent.click(screen.getByRole("button", { name: "切换金额显示" }));
  const input = screen.getByLabelText<HTMLInputElement>("当前金额");
  for (const value of ["-1", "10001", "abc", " ", ""]) {
    fireEvent.change(input, { target: { value } });
    expect(input.checkValidity(), value).toBe(false);
  }
  fireEvent.change(input, { target: { value: "0.000123456789" } });
  expect(input.checkValidity()).toBe(true);
  expect(input.value).toBe("0.000123456789");
  fireEvent.click(screen.getByRole("button", { name: "切换金额显示" }));
  expect(input.checkValidity()).toBe(true);
  expect(input.value).toBe("0.000123456789");
});
