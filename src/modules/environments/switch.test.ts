import { beforeEach, describe, expect, it, vi } from "vitest";

const store = new Map<string, string>();
globalThis.sessionStorage = {
  getItem: (key: string) => store.get(key) ?? null,
  setItem: (key: string, value: string) => void store.set(key, value),
  removeItem: (key: string) => void store.delete(key),
} as unknown as Storage;

describe("resumedFromSwitch", () => {
  beforeEach(() => { store.clear(); vi.resetModules(); });

  it("is false in a window that was not reloaded by a switch", async () => {
    const { resumedFromSwitch } = await import("./switch");
    expect(resumedFromSwitch()).toBe(false);
  });

  it("is true once the switch marked it, repeats the answer, and clears the mark", async () => {
    const { markSwitching, resumedFromSwitch } = await import("./switch");
    markSwitching();
    expect(resumedFromSwitch()).toBe(true);
    expect(resumedFromSwitch()).toBe(true);
    expect(store.size).toBe(0);
  });
});
