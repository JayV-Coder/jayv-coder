import { describe, expect, it } from "vitest";
import { fuzzyMatch, shortcutFor } from "./index";

const key = (value: string, extra: Partial<{ metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; isComposing: boolean }> = {}) =>
  ({ key: value, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...extra });

describe("shortcutFor", () => {
  it("usa ⌘ no Mac e Ctrl no resto", () => {
    expect(shortcutFor(key("k", { metaKey: true }), true)).toBe("palette");
    expect(shortcutFor(key("k", { ctrlKey: true }), true)).toBeNull();
    expect(shortcutFor(key("K", { ctrlKey: true }), false)).toBe("palette");
    expect(shortcutFor(key("k", { metaKey: true }), false)).toBeNull();
  });

  it("leva os números às telas principais", () => {
    expect(shortcutFor(key("1", { ctrlKey: true }), false)).toBe("projects");
    expect(shortcutFor(key("4", { ctrlKey: true }), false)).toBe("system");
    expect(shortcutFor(key(",", { ctrlKey: true }), false)).toBe("settings");
  });

  it("não rouba Shift, Alt, composição nem tecla sem atalho", () => {
    expect(shortcutFor(key("k", { ctrlKey: true, shiftKey: true }), false)).toBeNull();
    expect(shortcutFor(key("k", { ctrlKey: true, altKey: true }), false)).toBeNull();
    expect(shortcutFor(key("k", { ctrlKey: true, isComposing: true }), false)).toBeNull();
    expect(shortcutFor(key("z", { ctrlKey: true }), false)).toBeNull();
    expect(shortcutFor(key("k"), false)).toBeNull();
  });
});

describe("fuzzyMatch", () => {
  it("acha as letras em ordem e devolve onde", () => {
    expect(fuzzyMatch("nch", "New chat")?.positions).toEqual([0, 4, 5]);
    expect(fuzzyMatch("xyz", "New chat")).toBeNull();
    expect(fuzzyMatch("", "New chat")).toEqual({ score: 0, positions: [] });
  });

  it("prefere letras seguidas e começo de palavra", () => {
    const tight = fuzzyMatch("set", "Settings")!.score;
    const loose = fuzzyMatch("set", "Select theme")!.score;
    expect(tight).toBeGreaterThan(loose);
  });
});
