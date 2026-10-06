import { describe, expect, it } from "vitest";
import { fuzzyMatch, modeCommand, shortcutFor } from "./index";

const key = (value: string, extra: Partial<{ metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean; isComposing: boolean }> = {}) =>
  ({ key: value, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, ...extra });

describe("shortcutFor", () => {
  it("usa ⌘ no Mac e Ctrl no resto", () => {
    expect(shortcutFor(key("k", { metaKey: true }), true)).toBe("palette");
    expect(shortcutFor(key("k", { ctrlKey: true }), true)).toBeNull();
    expect(shortcutFor(key("K", { ctrlKey: true }), false)).toBe("palette");
    expect(shortcutFor(key("k", { metaKey: true }), false)).toBeNull();
  });

  it("Ctrl+F e ⌘F buscam na conversa", () => {
    expect(shortcutFor(key("f", { ctrlKey: true }), false)).toBe("find");
    expect(shortcutFor(key("F", { metaKey: true }), true)).toBe("find");
    expect(shortcutFor(key("f", { ctrlKey: true, shiftKey: true }), false)).toBeNull();
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

describe("modeCommand", () => {
  it("entende os três modos e os apelidos em português", () => {
    expect(modeCommand("/plan")).toEqual({ mode: "plan", rest: "" });
    expect(modeCommand("  /BUILD ")).toEqual({ mode: "build", rest: "" });
    expect(modeCommand("/auto")).toEqual({ mode: "auto", rest: "" });
    expect(modeCommand("/planejar")?.mode).toBe("plan");
    expect(modeCommand("/desenvolver")?.mode).toBe("build");
  });

  it("separa o pedido que vem junto do comando", () => {
    expect(modeCommand("/build adicione o teste do roteador\ne rode")).toEqual({ mode: "build", rest: "adicione o teste do roteador\ne rode" });
  });

  it("deixa passar o que não é comando de modo", () => {
    expect(modeCommand("/why")).toBeNull();
    expect(modeCommand("planeje /plan")).toBeNull();
    expect(modeCommand("/planning")).toBeNull();
    expect(modeCommand("/")).toBeNull();
  });
});

describe("atalho de modo", () => {
  it("Ctrl+. troca o modo do chat", () => {
    expect(shortcutFor(key(".", { ctrlKey: true }), false)).toBe("workMode");
  });
});
