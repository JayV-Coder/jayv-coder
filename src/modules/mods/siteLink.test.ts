import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));

const { readModsLink } = await import("./siteLink");

describe("the site's link to sync the mods", () => {
  it("reads the mod and the environment, only in the right shape", () => {
    expect(readModsLink("jayv://mods/sync?mod=ollama&environment=personal&at=1760650000000")).toEqual({ mod: "ollama", environment: "personal" });
    const organization = "0b6c1f3e-1d2a-4c5b-8e9f-123456789abc";
    expect(readModsLink(`jayv://mods/sync?mod=lm-studio&environment=${organization}`)).toEqual({ mod: "lm-studio", environment: organization });
    expect(readModsLink("jayv://mods/sync")).toEqual({ mod: null, environment: null });
    expect(readModsLink("jayv://mods/sync/?mod=../x&environment=root")).toEqual({ mod: null, environment: null });
  });

  it("leaves every other link alone", () => {
    expect(readModsLink("jayv://auth/callback?code=a")).toBeNull();
    expect(readModsLink("jayv://billing/done")).toBeNull();
    expect(readModsLink("jayv://mods/install?mod=ollama")).toBeNull();
    expect(readModsLink("https://mods/sync?mod=ollama")).toBeNull();
    expect(readModsLink("not a link")).toBeNull();
  });
});
