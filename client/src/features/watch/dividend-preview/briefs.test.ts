import { expect, it } from "vitest";
import { dividendBriefs } from "./briefs";
import { dividendCard } from "./card";
import { dividendQuestionSymbol } from "./route";
import snapshot from "./snapshot.json";

it("presents the same three ordered sections for every public sample", () => {
  expect(Object.keys(dividendBriefs).sort()).toEqual(
    snapshot.stocks.map(({ symbol }) => symbol).sort(),
  );
  for (const stock of snapshot.stocks) {
    const sections = dividendCard(stock)?.sections ?? [];
    expect(sections.map(({ title }) => title)).toEqual([
      "未来预期",
      "影响股息的关键数据",
      "分红与近一年股价",
    ]);
    for (const { kind, text } of sections) {
      expect(text.trim()).toBeTruthy();
      if (kind === "outlook") expect(text).not.toMatch(/[0-9０-９]/u);
    }
  }
});

it("accepts only a six-digit stock code from the public research link", () => {
  expect(dividendQuestionSymbol("?dividend=000333")).toBe("000333");
  expect(dividendQuestionSymbol("?dividend=000333.SZ")).toBeUndefined();
  expect(dividendQuestionSymbol("?dividend=000333%0A")).toBeUndefined();
  expect(
    dividendQuestionSymbol("?dividend=000333%0A另一个问题"),
  ).toBeUndefined();
});
