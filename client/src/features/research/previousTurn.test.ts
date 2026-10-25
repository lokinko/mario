import { expect, it } from "vitest";
import type { AnalysisResult } from "../../types";
import { previousTurn } from "./previousTurn";

it("carries only the prior conclusion and action text within the shared Unicode budget", () => {
  const turn = {
    question: "😀".repeat(300),
    result: {
      answer: "完整回答及不应发送的依据".repeat(1000),
      workflowTrace: {
        structuredReport: {
          answer: "😀".repeat(400),
          facts: [],
          question: "不携带的追问",
          suggestions: [1, 2, 3, 4].map(() => ({
            action: "😀".repeat(150),
            why: "不携带的解释",
            factIndices: [0],
            unlessWhen: "不携带的前提",
          })),
        },
      },
    } as unknown as AnalysisResult,
  };
  const result = previousTurn(turn)!;
  expect(
    [...result.question].length +
      [...result.answer].length +
      result.suggestions.reduce((sum, action) => sum + [...action].length, 0),
  ).toBe(800);
  expect(result.suggestions).toHaveLength(3);
  expect(JSON.stringify(result)).not.toContain("不携带");
  expect(JSON.stringify(result)).not.toContain("完整回答");
  expect(previousTurn()).toBeUndefined();
});

it("reads the frozen headline and actions of historical reports", () => {
  const result = previousTurn({
    question: "旧问题",
    result: {
      answer: "八个板块",
      workflowTrace: {
        structuredReport: {
          verdict: "旧结论",
          actions: [{ action: "旧动作" }],
        },
      },
    } as unknown as AnalysisResult,
  });
  expect(result).toEqual({
    question: "旧问题",
    answer: "旧结论",
    suggestions: ["旧动作"],
  });
});
