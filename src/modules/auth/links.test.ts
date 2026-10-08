import { describe, expect, it } from "vitest";
import { firstTime } from "./links";

const memory = () => {
  const data = new Map<string, string>();
  return { getItem: (key: string) => data.get(key) ?? null, setItem: (key: string, value: string) => void data.set(key, value) };
};

describe("firstTime", () => {
  it("treats a link as new once and as handled afterwards", () => {
    const storage = memory();
    expect(firstTime("jayv://auth/callback?code=a", storage)).toBe(true);
    expect(firstTime("jayv://auth/callback?code=a", storage)).toBe(false);
  });

  it("keeps a different link new", () => {
    const storage = memory();
    firstTime("jayv://auth/callback?code=a", storage);
    expect(firstTime("jayv://auth/callback?code=b", storage)).toBe(true);
  });

  it("falls back to new when there is no storage or it is broken", () => {
    expect(firstTime("jayv://x", null)).toBe(true);
    const broken = { getItem: () => { throw new Error("blocked"); }, setItem: () => undefined };
    expect(firstTime("jayv://x", broken)).toBe(true);
  });

  it("forgets the oldest links past twenty", () => {
    const storage = memory();
    for (let n = 0; n < 25; n += 1) firstTime(`jayv://l/${n}`, storage);
    expect(firstTime("jayv://l/24", storage)).toBe(false);
    expect(firstTime("jayv://l/0", storage)).toBe(true);
  });
});
