import { describe, expect, it, vi } from "vitest";

globalThis.localStorage = { getItem: () => null, setItem: () => undefined, removeItem: () => undefined } as unknown as Storage;

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("@/modules/auth/client", () => ({ supabase: {} }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));

const { environmentLabel, environmentOptions, PERSONAL } = await import("./index");

const orgs = [
  { id: "o2", name: "Zeta", role: "member" as const },
  { id: "o1", name: "Acme", role: "owner" as const },
];

describe("environmentOptions", () => {
  it("lists the personal environment first and the organizations by name", () => {
    expect(environmentOptions(orgs).map((item) => item.id)).toEqual([PERSONAL, "o1", "o2"]);
  });

  it("keeps the role of each organization and none for the personal one", () => {
    const [personal, acme] = environmentOptions(orgs);
    expect(personal).toEqual({ id: PERSONAL, kind: "personal", name: null, role: null });
    expect(acme).toEqual({ id: "o1", kind: "organization", name: "Acme", role: "owner" });
  });

  it("is only the personal environment without organizations", () => {
    expect(environmentOptions([]).map((item) => item.id)).toEqual([PERSONAL]);
  });
});

describe("environmentLabel", () => {
  it("names an organization or falls back to the personal text", () => {
    expect(environmentLabel("o1", orgs, "Personal")).toBe("Acme");
    expect(environmentLabel(PERSONAL, orgs, "Personal")).toBe("Personal");
    expect(environmentLabel("gone", orgs, "Personal")).toBe("Personal");
  });
});
