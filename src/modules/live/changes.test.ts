import { describe, expect, it } from "vitest";
import { withChange } from "./changes";

describe("withChange", () => {
  it("moves the file that just changed to the top, once", () => {
    const files = [
      { path: "a.rs", kind: "modified" as const, at: 1 },
      { path: "b.rs", kind: "created" as const, at: 2 },
    ];
    expect(withChange(files, { path: "b.rs", kind: "modified", at: 3 })).toEqual([
      { path: "b.rs", kind: "modified", at: 3 },
      { path: "a.rs", kind: "modified", at: 1 },
    ]);
  });
});
