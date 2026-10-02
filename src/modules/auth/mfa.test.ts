import { describe, expect, it } from "vitest";
import { needsSecondFactor, secretGroups, totpDigits, totpOk } from "./mfa";

describe("totp code", () => {
  it("keeps only six digits of what was typed or pasted", () => {
    expect(totpDigits("123 456")).toBe("123456");
    expect(totpDigits("1234567")).toBe("123456");
    expect(totpDigits("12a-34")).toBe("1234");
  });

  it("accepts exactly six digits", () => {
    expect(totpOk("123456")).toBe(true);
    expect(totpOk("12345")).toBe(false);
    expect(totpOk("1234567")).toBe(false);
    expect(totpOk("12345a")).toBe(false);
  });
});

describe("needsSecondFactor", () => {
  it("asks for the code only when the account has a factor the session lacks", () => {
    expect(needsSecondFactor({ currentLevel: "aal1", nextLevel: "aal2" })).toBe(true);
    expect(needsSecondFactor({ currentLevel: "aal2", nextLevel: "aal2" })).toBe(false);
    expect(needsSecondFactor({ currentLevel: "aal1", nextLevel: "aal1" })).toBe(false);
    expect(needsSecondFactor(null)).toBe(false);
  });
});

describe("secretGroups", () => {
  it("splits the key in blocks of four", () => {
    expect(secretGroups("ABCDEFGHIJ")).toBe("ABCD EFGH IJ");
    expect(secretGroups("")).toBe("");
  });
});
