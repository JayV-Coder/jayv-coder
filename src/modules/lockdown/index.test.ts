import { describe, expect, it } from "vitest";
import { isBlockedShortcut, keepsContextMenu, type KeyPress } from "./index";

const press = (key: string, mods: Partial<KeyPress> = {}): KeyPress => ({ key, ctrlKey: false, metaKey: false, shiftKey: false, altKey: false, ...mods });

describe("isBlockedShortcut", () => {
  it("blocks the inspector, view source and reload shortcuts", () => {
    expect(isBlockedShortcut(press("F12"))).toBe(true);
    expect(isBlockedShortcut(press("I", { ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("i", { metaKey: true, altKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("u", { ctrlKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("r", { metaKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("F5"))).toBe(true);
  });

  it("keeps editing shortcuts", () => {
    expect(isBlockedShortcut(press("c", { ctrlKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("v", { metaKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("a", { ctrlKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("i"))).toBe(false);
    expect(isBlockedShortcut(press("Enter", { shiftKey: true }))).toBe(false);
  });
});

describe("keepsContextMenu", () => {
  it("is false without an element", () => {
    expect(keepsContextMenu(null)).toBe(false);
  });
});
