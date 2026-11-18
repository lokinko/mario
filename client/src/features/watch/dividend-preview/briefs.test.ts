import { expect, it } from "vitest";
import { dividendBriefs } from "./briefs";
import { dividendQuestionSymbol } from "./route";
import snapshot from "./snapshot.json";

it("pairs each public sample with a share-card headline and a research question", () => {
  expect(Object.keys(dividendBriefs).sort()).toEqual(
    snapshot.stocks.map(({ symbol }) => symbol).sort(),
  );
  for (const brief of Object.values(dividendBriefs)) {
    expect(brief?.headline.length).toBeGreaterThan(0);
    expect(brief?.headline.length).toBeLessThanOrEqual(20);
    expect(brief?.reading).toBeTruthy();
    expect(brief?.followUp).toBeTruthy();
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
