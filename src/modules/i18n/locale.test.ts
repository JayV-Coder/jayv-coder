import { describe, expect, it } from "vitest";
import { replyName, resolveLocale } from "./locale";

const locales = [{ id: "en", name: "English" }, { id: "pt-BR", name: "Português" }, { id: "zh-CN", name: "中文" }];
const base = { locales, listed: true, system: [] as string[], fallback: "en" };

describe("resolveLocale", () => {
  it("keeps the saved language when the list knows it", () => {
    expect(resolveLocale({ ...base, saved: "pt-BR" })).toBe("pt-BR");
  });

  it("keeps the saved language when the list never arrived", () => {
    expect(resolveLocale({ ...base, saved: "pt-BR", locales: [{ id: "en", name: "English" }], listed: false })).toBe("pt-BR");
  });

  it("falls back to the closest system language, then to the fallback", () => {
    expect(resolveLocale({ ...base, saved: null, system: ["pt-PT"] })).toBe("pt-BR");
    expect(resolveLocale({ ...base, saved: "xx", system: ["zh-TW"] })).toBe("zh-CN");
    expect(resolveLocale({ ...base, saved: null, system: ["fi"] })).toBe("en");
  });
});

describe("replyName", () => {
  it("uses the name in the list and the browser's name for an unlisted code", () => {
    expect(replyName("pt-BR", locales)).toBe("Português");
    expect(replyName("fr", locales)).toBe("French");
  });
});
