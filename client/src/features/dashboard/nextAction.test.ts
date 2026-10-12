import { expect, it } from "vitest";
import { nextAction } from "./nextAction";
import type { Goal, ReviewReminderSummary, Snapshot } from "../../types";
const snapshot: Pick<Snapshot, "goals" | "valuationStatus"> = {
  goals: [{ id: "goal" } as Goal],
  valuationStatus: {
    comparable: true,
    baseCurrency: "CNY",
    warnings: [],
    missingFxHoldings: [],
    undatedHoldingCount: 0,
    valuationDates: [],
    alignedValuationDate: null,
  },
};
const reminder: ReviewReminderSummary = {
  periodicReviewDue: false,
  checkedOn: "2026-09-09",
};
it("does not equate missing reminders with no work", () => {
  expect(nextAction(snapshot, null).destination).toBe("review");
});
it("offers no action when checked prerequisites and reviews are satisfied", () => {
  expect(nextAction(snapshot, reminder).destination).toBeUndefined();
});
it("prioritizes prerequisites, then due reviews, without model or account inputs", () => {
  expect(nextAction({ ...snapshot, goals: [] }, reminder).destination).toBe(
    "foundation",
  );
  expect(
    nextAction(
      {
        ...snapshot,
        valuationStatus: { ...snapshot.valuationStatus, comparable: false },
      },
      reminder,
    ).title,
  ).toContain("估值");
  expect(
    nextAction(snapshot, { ...reminder, periodicReviewDue: true }).label,
  ).toBe("开始周期复盘");
});
