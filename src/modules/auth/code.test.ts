import { describe, expect, it } from "vitest";
import { codeDigits, codeOk } from "./code";

describe("codeOk", () => {
  it("aceita de 6 a 10 dígitos", () => {
    expect(codeOk("123456")).toBe(true);
    expect(codeOk("12345678")).toBe(true);
    expect(codeOk("1234567890")).toBe(true);
  });
  it("recusa curto, longo ou com letras", () => {
    expect(codeOk("12345")).toBe(false);
    expect(codeOk("12345678901")).toBe(false);
    expect(codeOk("12345a")).toBe(false);
  });
});

describe("codeDigits", () => {
  it("guarda o código de 8 dígitos colado com espaços inteiro", () => {
    expect(codeDigits(" 1234 5678 ")).toBe("12345678");
  });
  it("corta além do máximo", () => {
    expect(codeDigits("123456789012")).toBe("1234567890");
  });
});
