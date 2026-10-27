import { describe, expect, it } from "vitest";
import { EMPTY, mark, nextStep, parse, secondsToVerified, showing } from "./progress";

describe("first steps", () => {
  it("marks steps in order and times the first verified result", () => {
    let progress = { ...EMPTY, startedAt: 1_000 };
    progress = mark(progress, "folder", 2_000);
    expect(nextStep(progress)).toBe("request");
    progress = mark(progress, "verified", 181_000);
    expect(progress.done.request).toBe(181_000);
    expect(nextStep(progress)).toBeNull();
    expect(secondsToVerified(progress)).toBe(180);
    expect(showing(progress)).toBe(false);
  });

  it("keeps the first time a step happened", () => {
    const once = mark(EMPTY, "folder", 5);
    expect(mark(once, "folder", 9)).toBe(once);
    expect(once.startedAt).toBe(5);
  });

  it("reads only well-formed progress", () => {
    expect(parse("not json")).toEqual(EMPTY);
    expect(parse(JSON.stringify({ startedAt: 1, done: { folder: 2, request: "x" }, dismissed: true }))).toEqual({ startedAt: 1, done: { folder: 2 }, dismissed: true, reported: false });
    expect(showing(parse(JSON.stringify({ dismissed: true })))).toBe(false);
  });
});
