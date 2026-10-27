import { describe, expect, it } from "vitest";
import { Counter } from "./counter";

describe("feature usage counter", () => {
  it("counts per feature and UTC day, and drains everything", () => {
    const counter = new Counter();
    counter.add("stats", new Date("2026-10-27T23:30:00Z"));
    counter.add("stats", new Date("2026-10-27T10:00:00Z"));
    counter.add("mcp", new Date("2026-10-28T00:10:00Z"));
    expect(counter.drain()).toEqual([
      { feature: "stats", day: "2026-10-27", uses: 2 },
      { feature: "mcp", day: "2026-10-28", uses: 1 },
    ]);
    expect(counter.size).toBe(0);
  });

  it("gives back what failed to send, added to what came after", () => {
    const counter = new Counter();
    counter.add("stats", new Date("2026-10-27T10:00:00Z"));
    const sent = counter.drain();
    counter.add("stats", new Date("2026-10-27T11:00:00Z"));
    counter.restore(sent);
    expect(counter.drain()).toEqual([{ feature: "stats", day: "2026-10-27", uses: 2 }]);
  });
});
