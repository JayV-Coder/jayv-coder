import { describe, expect, it, vi } from "vitest";

vi.mock("@/modules/auth/client", () => ({ supabase: {} }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@/modules/i18n", () => ({ t: (key: string) => key }));
vi.mock("@/modules/feedback", () => ({ notify: vi.fn(), reportError: vi.fn() }));

const { allows, currenciesOf, formatPrice, locks, priceIn, regionOf, toggleState } = await import("./index");

describe("recursos do plano", () => {
  it("sem lista lida, vale tudo", () => {
    expect(allows({ features: null }, "stats")).toBe(true);
  });
  it("com lista, só o que está nela", () => {
    const state = { plan: "free", admin: false, features: new Set(["stats"]) };
    expect(allows(state, "stats")).toBe(true);
    expect(allows(state, "secondOpinion")).toBe(false);
  });
  it("o núcleo vale sempre, com lista ou sem", () => {
    expect(allows({ features: new Set(["stats"]) }, "secretRedaction")).toBe(true);
    expect(locks({}, "agentSessions")).toBe(true);
    expect(locks({ locked: new Set(["parallelTasks", "agentSessions"]) }, "parallelTasks")).toBe(true);
  });
  it("o interruptor aparece travado, fora do plano ou livre", () => {
    const team = { features: new Set(["secondOpinion"]), locked: new Set(["agentSessions", "parallelTasks"]) };
    expect(toggleState(team, "parallelTasks", false)).toEqual({ checked: true, disabled: true, reason: "required" });
    expect(toggleState(team, "planFirst", true)).toEqual({ checked: true, disabled: true, reason: "outside" });
    expect(toggleState(team, "secondOpinion", false)).toEqual({ checked: false, disabled: false, reason: null });
  });
  it("o preço sai na moeda e no idioma da tela", () => {
    expect(formatPrice({ priceCents: 4900, currency: "brl" }, "pt-BR")).toBe("R$ 49,00");
    expect(formatPrice({ priceCents: null, currency: "brl" }, "pt-BR")).toBeNull();
  });
});

describe("preço em várias moedas", () => {
  const pro = { isDefault: false, priceCents: 4900, currency: "brl", extraPrices: [{ currency: "usd", priceCents: 990 }, { currency: "eur", priceCents: 900 }] };
  const free = { isDefault: true, priceCents: null, currency: null, extraPrices: [] };
  it("lista as moedas que algum plano vende, a padrão primeiro", () => {
    expect(currenciesOf([free, pro])).toEqual(["brl", "usd", "eur"]);
    expect(currenciesOf([free])).toEqual([]);
  });
  it("mostra o preço na moeda escolhida, ou no padrão se o plano não a vende", () => {
    expect(priceIn(pro, "usd")).toEqual({ priceCents: 990, currency: "usd" });
    expect(priceIn(pro, "gbp")).toEqual({ priceCents: 4900, currency: "brl" });
    expect(priceIn(pro, null)).toEqual({ priceCents: 4900, currency: "brl" });
  });
  it("lê a região do idioma do sistema", () => {
    expect(regionOf("pt-BR")).toBe("BR");
    expect(regionOf("zh-Hans-CN")).toBe("CN");
    expect(regionOf("en")).toBeNull();
  });
});
