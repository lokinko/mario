// @vitest-environment jsdom
import { useState } from "react";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ProfileForm } from "./ProfileForm";
import { saveProfile } from "../../api/portfolio";
import type { FinancialProfile, Snapshot } from "../../types";
import fixture from "../../test/emptySnapshot.json";

vi.mock("../../api/portfolio", () => ({ saveProfile: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

function mount(initial = fixture.profile as FinancialProfile) {
  const onUpdate = vi.fn();
  function Form() {
    const [profile, setProfile] = useState(initial);
    const [error, setError] = useState("");
    return (
      <>
        <ProfileForm
          profile={profile}
          onChange={setProfile}
          savedBaseCurrency="CNY"
          busy={false}
          onUpdate={onUpdate}
          flash={vi.fn()}
          run={async (task) => {
            try {
              await task();
            } catch {
              setError("保存失败");
            }
          }}
        />
        {error && <p>{error}</p>}
      </>
    );
  }
  render(<Form />);
  return onUpdate;
}

function input(name: string) {
  return screen.getByRole("spinbutton", { name }) as HTMLInputElement;
}
function save() {
  fireEvent.click(screen.getByRole("button", { name: "保存收支" }));
}

it("keeps unknown fields blank and saves explicit zero and former defaults as provided facts", async () => {
  vi.mocked(saveProfile).mockImplementation(
    async (profile) => ({ ...fixture, profile }) as Snapshot,
  );
  const onUpdate = mount();
  for (const field of screen.getAllByRole("spinbutton")) {
    expect((field as HTMLInputElement).value).toBe("");
  }
  fireEvent.change(input("投资期限 年"), { target: { value: "5" } });
  fireEvent.change(input("负债 CNY"), { target: { value: "0" } });
  fireEvent.change(input("可承受最大回撤 %"), { target: { value: "15" } });
  expect(saveProfile).not.toHaveBeenCalled();
  save();
  await waitFor(() => expect(onUpdate).toHaveBeenCalledOnce());
  expect(saveProfile).toHaveBeenCalledWith(
    expect.objectContaining({
      horizonYears: 5,
      liabilities: 0,
      maxDrawdownPct: 15,
      providedFields: ["liabilities", "horizonYears", "maxDrawdownPct"],
    }),
  );
  expect(input("投资期限 年").value).toBe("5");
  expect(input("负债 CNY").value).toBe("0");
  expect(input("月收入 CNY").value).toBe("");
});

it("clears individual saved facts without erasing untouched exact values", async () => {
  vi.mocked(saveProfile).mockImplementation(
    async (profile) => ({ ...fixture, profile }) as Snapshot,
  );
  mount({
    ...fixture.profile,
    providedFields: [
      "monthlyIncome",
      "liabilities",
      "horizonYears",
      "maxDrawdownPct",
    ],
    monthlyIncome: 123.12345678,
    exactAmounts: {
      monthlyIncome: "123.12345678",
      monthlyExpense: "0",
      emergencyFund: "0",
      liabilities: "0",
    },
  });
  fireEvent.change(input("负债 CNY"), { target: { value: "" } });
  fireEvent.change(input("投资期限 年"), { target: { value: "" } });
  fireEvent.change(input("可承受最大回撤 %"), { target: { value: "" } });
  save();
  await waitFor(() =>
    expect(saveProfile).toHaveBeenCalledWith(
      expect.objectContaining({
        providedFields: ["monthlyIncome"],
        exactAmounts: expect.objectContaining({
          monthlyIncome: "123.12345678",
        }),
      }),
    ),
  );
  expect(input("负债 CNY").value).toBe("");
  expect(input("投资期限 年").value).toBe("");
  expect(input("可承受最大回撤 %").value).toBe("");
});

it("preserves legacy nondefault facts and the draft after a failed save", async () => {
  vi.mocked(saveProfile).mockRejectedValueOnce(new Error("offline"));
  const onUpdate = mount({
    ...fixture.profile,
    monthlyIncome: 8000,
    horizonYears: 12,
  });
  expect(input("月收入 CNY").value).toBe("8000");
  expect(input("投资期限 年").value).toBe("12");
  expect(input("负债 CNY").value).toBe("");
  fireEvent.change(input("投资期限 年"), { target: { value: "0" } });
  save();
  await screen.findByText("保存失败");
  expect(onUpdate).not.toHaveBeenCalled();
  expect(input("投资期限 年").value).toBe("0");
  vi.mocked(saveProfile).mockImplementation(
    async (profile) => ({ ...fixture, profile }) as Snapshot,
  );
  save();
  await waitFor(() => expect(onUpdate).toHaveBeenCalledOnce());
  expect(saveProfile).toHaveBeenLastCalledWith(
    expect.objectContaining({
      horizonYears: 0,
      providedFields: ["monthlyIncome", "horizonYears"],
    }),
  );
});
