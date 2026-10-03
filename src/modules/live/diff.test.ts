import { describe, expect, it } from "vitest";
import { diffCounts, firstChangedLine, hunks, lineDiff } from "./diff";

const shape = (before: string, after: string) => lineDiff(before, after).map((line) => `${line.kind[0]}${line.text}`);

describe("lineDiff", () => {
  it("marks added and removed lines with their numbers", () => {
    const lines = lineDiff("a\nb\nc\n", "a\nB\nc\nd\n");
    expect(lines.map((line) => [line.kind, line.text, line.before, line.after])).toEqual([
      ["same", "a", 1, 1],
      ["removed", "b", 2, null],
      ["added", "B", null, 2],
      ["same", "c", 3, 3],
      ["added", "d", null, 4],
    ]);
    expect(diffCounts(lines)).toEqual({ added: 2, removed: 1 });
    expect(firstChangedLine(lines)).toBe(2);
  });

  it("handles new and deleted files", () => {
    expect(shape("", "x\ny")).toEqual(["ax", "ay"]);
    expect(shape("x\n", "")).toEqual(["rx"]);
  });

  it("finds the shortest edit in the middle", () => {
    expect(shape("a\nx\ny\nb", "a\ny\nz\nb")).toEqual(["sa", "rx", "sy", "az", "sb"]);
  });

  it("gives up gracefully past the limit", () => {
    const lines = lineDiff("a\nb\nc", "x\ny\nz", 1);
    expect(diffCounts(lines)).toEqual({ added: 3, removed: 3 });
  });
});

describe("hunks", () => {
  it("keeps three lines of context and merges close changes", () => {
    const before = Array.from({ length: 30 }, (_, index) => `l${index + 1}`).join("\n");
    const after = before.replace("l5", "L5").replace("l8", "L8").replace("l25", "L25");
    const parts = hunks(lineDiff(before, after));
    expect(parts).toHaveLength(2);
    expect(parts[0].skipped).toBe(1);
    expect(parts[0].lines[0].text).toBe("l2");
    expect(parts[1].lines.some((line) => line.text === "L25")).toBe(true);
  });

  it("is empty when nothing changed", () => {
    expect(hunks(lineDiff("a\nb", "a\nb"))).toEqual([]);
  });
});
