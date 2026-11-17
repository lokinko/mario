import { expect, it } from "vitest";
import type { AnalysisResult, PreviousTurn } from "../../types";
import { previousTurn } from "./previousTurn";

const turn = (question: string, frozen?: PreviousTurn) => ({
  question,
  result: {
    answer: "AI 正文中的用户问题不应被恢复",
    workflowTrace: { personalContext: { previousTurn: frozen } },
  } as unknown as AnalysisResult,
});

it("keeps the original 800 Unicode budget and independently clips the structured pending question", () => {
  const current = {
    question: "😀".repeat(300),
    result: {
      answer: "完整回答及不应发送的依据".repeat(1000),
      workflowTrace: {
        structuredReport: {
          answer: "😀".repeat(400),
          facts: [],
          question: "🧐".repeat(250),
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
  const result = previousTurn([current])!;
  expect(
    [...result.question].length +
      [...result.answer].length +
      result.suggestions.reduce((sum, action) => sum + [...action].length, 0),
  ).toBe(800);
  expect([...result.pendingQuestion!]).toHaveLength(200);
  expect(result.suggestions).toHaveLength(3);
  expect(result.earlierQuestions).toBeUndefined();
  expect(JSON.stringify(result)).not.toContain("不携带");
  expect(JSON.stringify(result)).not.toContain("完整回答");
  expect(previousTurn()).toBeUndefined();
  expect(previousTurn([])).toBeUndefined();
});

it("reads the frozen headline and actions of historical reports without inventing a pending question", () => {
  const result = previousTurn([
    {
      question: "旧问题",
      result: {
        answer: "八个板块，其中有 AI 自行提出的问题？",
        workflowTrace: {
          structuredReport: {
            verdict: "旧结论",
            actions: [{ action: "旧动作" }],
            unknowns: ["不是用户问题或模型结构化追问"],
          },
        },
      } as unknown as AnalysisResult,
    },
  ]);
  expect(result).toEqual({
    question: "旧问题",
    answer: "旧结论",
    suggestions: ["旧动作"],
  });
});

it("carries three or more completed user turns in chronological order and preserves the first plus latest four", () => {
  const questions = Array.from(
    { length: 9 },
    (_, index) => `用户问题${index + 1}`,
  );
  const turns = questions.map((question) => turn(question));
  expect(previousTurn(turns.slice(0, 3))?.earlierQuestions).toEqual(
    questions.slice(0, 2),
  );
  expect(previousTurn(turns)?.earlierQuestions).toEqual([
    questions[0],
    ...questions.slice(4, 8),
  ]);
  expect(previousTurn(turns)?.question).toBe(questions[8]);
  expect(JSON.stringify(previousTurn(turns)?.earlierQuestions)).not.toContain(
    "AI 正文中的用户问题",
  );
});

it("deduplicates user questions, excludes the previous question and clips earlier questions by Unicode characters", () => {
  const first = "😀".repeat(350);
  const turns = [
    first,
    "重复问题",
    "第二个问题",
    "重复问题",
    "当前上轮问题",
    "当前上轮问题",
  ].map((question) => turn(question));
  const result = previousTurn(turns)!;
  expect(result.earlierQuestions).toEqual([
    "😀".repeat(300),
    "第二个问题",
    "重复问题",
  ]);
  expect([...result.earlierQuestions![0]]).toHaveLength(300);
  expect(result.earlierQuestions).not.toContain(result.question);
  expect(
    previousTurn([turn(first), turn(first)])?.earlierQuestions,
  ).toBeUndefined();
});

it("retains a repeated recent question by its latest occurrence while preserving the first question", () => {
  const questions = [
    "首轮",
    "近期重复",
    "第三轮",
    "第四轮",
    "第五轮",
    "第六轮",
    "近期重复",
    "首轮",
    "上一轮",
  ];
  expect(
    previousTurn(questions.map((question) => turn(question)))?.earlierQuestions,
  ).toEqual(["首轮", "第四轮", "第五轮", "第六轮", "近期重复"]);
});

it("recovers early user questions from the first reopened report and retains them through later live turns", () => {
  const frozen = {
    question: "历史上一轮用户问题",
    answer: "旧摘要",
    suggestions: [],
    earlierQuestions: ["首轮用户问题", "历史第二轮用户问题", "首轮用户问题"],
    pendingQuestion: "更早模型的追问不得延续",
  };
  const stored = turn("历史当前用户问题", frozen);
  expect(previousTurn([stored])).toEqual({
    question: stored.question,
    answer: stored.result.answer.slice(0, 300),
    suggestions: [],
    earlierQuestions: [
      "首轮用户问题",
      "历史第二轮用户问题",
      "历史上一轮用户问题",
    ],
  });
  expect(
    previousTurn([stored, turn("刚完成的用户问题")])?.earlierQuestions,
  ).toEqual([
    "首轮用户问题",
    "历史第二轮用户问题",
    "历史上一轮用户问题",
    "历史当前用户问题",
  ]);
  expect(
    previousTurn([
      turn("旧契约历史问题", {
        question: "旧契约首轮问题",
        answer: "旧摘要",
        suggestions: [],
      }),
    ])?.earlierQuestions,
  ).toEqual(["旧契约首轮问题"]);
});

it("does not add truncated copies from subsequent frozen reports when original live questions are available", () => {
  const longQuestion = "长".repeat(300);
  const turns = [
    turn(longQuestion),
    turn("最近问题", {
      question: longQuestion.slice(0, 200),
      answer: "摘要",
      suggestions: [],
    }),
    turn("最新问题"),
  ];
  expect(previousTurn(turns)?.earlierQuestions).toEqual([
    longQuestion,
    "最近问题",
  ]);
});

it.each([null, "", "   "])(
  "omits an absent structured pending question (%s)",
  (question) => {
    const current = turn("用户原始问题");
    current.result.workflowTrace.structuredReport = {
      answer: "结构化回答",
      suggestions: [],
      facts: [],
      question,
    };
    expect(previousTurn([current])?.pendingQuestion).toBeUndefined();
  },
);
