import { describe, expect, it } from "vitest";
import { matchSpans, stepIndex } from "./find";

describe("matchSpans", () => {
  it("finds every occurrence regardless of case and accents", () => {
    expect(matchSpans("Plano do PLANO e do planó", "plano")).toEqual([[0, 5], [9, 14], [20, 25]]);
  });

  it("returns spans in the original text, not the folded one", () => {
    const text = "café açaí café";
    const [first, second] = matchSpans(text, "cafe");
    expect(text.slice(...first)).toBe("café");
    expect(text.slice(...second)).toBe("café");
  });

  it("ignores empty searches and does not overlap matches", () => {
    expect(matchSpans("abc", "  ")).toEqual([]);
    expect(matchSpans("aaaa", "aa")).toEqual([[0, 2], [2, 4]]);
  });

  it("handles characters outside the basic plane", () => {
    const text = "a 😀 b 😀";
    expect(matchSpans(text, "😀").map(([from, to]) => text.slice(from, to))).toEqual(["😀", "😀"]);
  });
});

describe("stepIndex", () => {
  it("wraps around both ends", () => {
    expect(stepIndex(0, 3, -1)).toBe(2);
    expect(stepIndex(2, 3, 1)).toBe(0);
    expect(stepIndex(1, 3, 1)).toBe(2);
    expect(stepIndex(0, 0, 1)).toBe(0);
  });
});
