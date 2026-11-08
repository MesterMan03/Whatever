import { test, expect } from "vitest";
import { distribution, geometricMean, score } from "../../games/test/mods/markbench/scripts/scoring.ts";
test("Markbench scores use nearest-rank percentiles, geometric aggregation and no missing samples", () => {
  expect(distribution([1, 2, 3, 4]).p95).toBe(4);
  expect(distribution([]).p95).toBeNull();
  expect(score(1000 / 60)).toBeCloseTo(1000);
  expect(score(null)).toBeNull();
  expect(geometricMean([100, 400])).toBeCloseTo(200);
});
