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
    expect(isBlockedShortcut(press("R", { ctrlKey: true, shiftKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("F5"))).toBe(true);
  });

  it("blocks print, save, native find, open file, history and downloads", () => {
    for (const key of ["p", "s", "f", "g", "o", "h", "j", "d", "l"]) expect(isBlockedShortcut(press(key, { ctrlKey: true })), key).toBe(true);
    expect(isBlockedShortcut(press("F3"))).toBe(true);
    expect(isBlockedShortcut(press("g", { metaKey: true, shiftKey: true }))).toBe(true);
  });

  it("blocks zoom, back and forward, tabs and windows", () => {
    for (const key of ["+", "=", "-", "0"]) expect(isBlockedShortcut(press(key, { ctrlKey: true })), key).toBe(true);
    expect(isBlockedShortcut(press("ArrowLeft", { altKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("ArrowRight", { altKey: true }))).toBe(true);
    expect(isBlockedShortcut(press("BrowserBack"))).toBe(true);
    for (const key of ["t", "w", "n", "Tab", "PageDown"]) expect(isBlockedShortcut(press(key, { ctrlKey: true })), key).toBe(true);
    expect(isBlockedShortcut(press("F11"))).toBe(true);
  });

  it("keeps text editing", () => {
    for (const key of ["c", "x", "v", "a", "z", "y"]) expect(isBlockedShortcut(press(key, { ctrlKey: true })), key).toBe(false);
    expect(isBlockedShortcut(press("z", { ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("v", { metaKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("ArrowLeft"))).toBe(false);
    expect(isBlockedShortcut(press("ArrowLeft", { ctrlKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("i"))).toBe(false);
    expect(isBlockedShortcut(press("Enter", { shiftKey: true }))).toBe(false);
    expect(isBlockedShortcut(press("Tab"))).toBe(false);
  });

  it("does not take the shortcuts the app listens to", () => {
    // O bloqueio cancela o efeito nativo; o app continua ouvindo estas teclas
    // (o teste de `shortcutFor` confere o mapa), então o que sobra livre aqui é
    // o que o app não usa.
    for (const key of ["k", "1", "2", "3", "4", ",", "."]) expect(isBlockedShortcut(press(key, { ctrlKey: true })), key).toBe(false);
  });
});

describe("keepsContextMenu", () => {
  it("is false without an element", () => {
    expect(keepsContextMenu(null)).toBe(false);
  });
});
