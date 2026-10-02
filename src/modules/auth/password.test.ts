import { describe, expect, it } from "vitest";
import { passwordOk, passwordRules } from "./password";

describe("passwordRules", () => {
  it("marca cada regra", () => {
    expect(passwordRules("abc")).toEqual({ length: false, lower: true, upper: false, digit: false, symbol: false });
    expect(passwordRules("Abcdefg1!")).toEqual({ length: true, lower: true, upper: true, digit: true, symbol: true });
  });
  it("letra acentuada é letra, espaço é símbolo", () => {
    expect(passwordRules("Ébcdefg1ç").symbol).toBe(false);
    expect(passwordRules("Ébcdefg1ç").upper).toBe(true);
    expect(passwordRules("Abcdefg1 ").symbol).toBe(true);
  });
  it("passwordOk exige todas", () => {
    expect(passwordOk("Abcdefg1!")).toBe(true);
    expect(passwordOk("Abcdefgh!")).toBe(false);
    expect(passwordOk("Ab1!")).toBe(false);
  });
});
