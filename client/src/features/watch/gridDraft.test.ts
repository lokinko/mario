import { expect, it } from "vitest";
import {
  generatePriceLevels,
  priceAtYield,
  yieldText,
  type PriceGridGenerator,
} from "./gridDraft";

const generator: PriceGridGenerator = {
  basePrice: "10",
  stepUnit: "pct",
  step: "10",
  down: "3",
  up: "2",
  buySize: "10",
  sellSize: "5",
};
it("expands geometric or fixed steps above and below the base price", () => {
  expect(generatePriceLevels(generator)).toEqual([
    { price: "12.10", size: "5" },
    { price: "11.00", size: "5" },
    { price: "9.00", size: "10" },
    { price: "8.10", size: "10" },
    { price: "7.29", size: "10" },
  ]);
  expect(
    generatePriceLevels({
      ...generator,
      stepUnit: "price",
      step: "0.5",
      up: "0",
    }).map((l) => l.price),
  ).toEqual(["9.50", "9.00", "8.50"]);
});
it("rejects missing bases, empty sides, collapsing steps and negative prices", () => {
  for (const change of [
    { basePrice: "" },
    { down: "0" },
    { down: "1.5" },
    { step: "100" },
    { step: "0.01" },
    { stepUnit: "price" as const, step: "4" },
    { up: "99" },
  ])
    expect(() => generatePriceLevels({ ...generator, ...change })).toThrow();
});
it("converts between price and the TTM dividend yield", () => {
  expect(yieldText(0.5, "10")).toBe("5");
  expect(priceAtYield(0.5, "6")).toBe("8.33");
  expect(yieldText(null, "10")).toBe("");
  expect(priceAtYield(0.5, "0")).toBe("");
});
