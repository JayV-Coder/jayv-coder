import { describe, expect, it } from "vitest";
import { FEATURES, isFeature, SETTINGS_TAB_FEATURE, VIEW_FEATURE } from "./catalog";

describe("catálogo de recursos", () => {
  it("reconhece só as chaves do catálogo", () => {
    expect(isFeature("stats")).toBe(true);
    expect(isFeature("nada")).toBe(false);
  });
  it("toda tela travada aponta para um recurso do catálogo", () => {
    for (const feature of Object.values(VIEW_FEATURE)) expect(FEATURES).toContain(feature);
  });
  it("toda aba travada das configurações aponta para um recurso do catálogo", () => {
    for (const feature of Object.values(SETTINGS_TAB_FEATURE)) expect(FEATURES).toContain(feature);
  });
  it("as chaves seguem o formato aceito pela tabela features", () => {
    for (const feature of FEATURES) expect(feature).toMatch(/^[a-z][A-Za-z0-9]{1,40}$/);
  });
});
