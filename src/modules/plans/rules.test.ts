import { describe, expect, it, vi } from "vitest";

vi.mock("@/modules/auth/client", () => ({ supabase: {} }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@/modules/i18n", () => ({ t: (key: string) => key }));
vi.mock("@/modules/feedback", () => ({ notify: vi.fn(), reportError: vi.fn() }));

const { allows, formatPrice } = await import("./index");

describe("recursos do plano", () => {
  it("sem lista lida, vale tudo", () => {
    expect(allows({ plan: null, admin: false, features: null }, "stats")).toBe(true);
  });
  it("com lista, só o que está nela", () => {
    const state = { plan: "free", admin: false, features: new Set(["stats"]) };
    expect(allows(state, "stats")).toBe(true);
    expect(allows(state, "secondOpinion")).toBe(false);
  });
  it("o preço sai na moeda e no idioma da tela", () => {
    expect(formatPrice({ priceCents: 4900, currency: "brl" }, "pt-BR")).toBe("R$ 49,00");
    expect(formatPrice({ priceCents: null, currency: "brl" }, "pt-BR")).toBeNull();
  });
});
