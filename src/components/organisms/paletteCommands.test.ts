import { describe, expect, it, vi } from "vitest";

const storage = { getItem: () => null, setItem: () => undefined, removeItem: () => undefined };
vi.stubGlobal("localStorage", storage);
const media = () => ({ matches: false, addEventListener: () => undefined, removeEventListener: () => undefined });
vi.stubGlobal("matchMedia", media);
vi.stubGlobal("window", { matchMedia: media, localStorage: storage, addEventListener: () => undefined });
vi.stubGlobal("document", { documentElement: { classList: { toggle: () => undefined, add: () => undefined, remove: () => undefined }, dataset: {}, style: {}, setAttribute: () => undefined } });
vi.mock("@/modules/auth/client", () => ({ supabase: {} }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn() }));

const { paletteCommands } = await import("./paletteCommands");
const { translate } = await import("@/modules/i18n");
type Key = Parameters<typeof translate>[1];

const project = { id: "p1", name: "App", rootPath: "/work/app", createdAt: "2026-10-01T00:00:00Z", orgId: null };
const chat = { id: "c1", projectId: "p1", title: "", updatedAt: "2026-10-02T10:00:00Z", workMode: "auto", turns: [] };
const context = (patch = {}) => ({
  t: (key: Key, params?: Record<string, string | number>) => translate("en", key, params),
  data: { projects: [project], chats: [chat] } as never,
  project: project as never,
  activeChatId: "c1",
  view: "chat" as const,
  layout: "grid" as const,
  locale: "en",
  locales: [{ id: "en", name: "English" }, { id: "pt-BR", name: "Português" }],
  organizations: [{ id: "o1", name: "Acme", slug: "acme", role: "owner" as const, members: 1, repositories: 1 }],
  invites: [],
  openOrganizationId: "o1",
  settingsDirty: true,
  rights: { features: null, locked: new Set<string>() },
  ...patch,
});
const ids = (patch = {}) => paletteCommands(context(patch)).map((command) => command.id);

describe("paletteCommands", () => {
  it("reaches every screen, every settings tab and the project actions", () => {
    const found = ids();
    for (const id of ["projects", "organizations", "stats", "system", "settings", "profile", "plans", "gate", "chats", "new-chat", "new-project"]) expect(found).toContain(id);
    for (const tab of ["app", "jev", "mcp", "skills", "claude", "codex", "copilot", "cursor"]) expect(found).toContain(`settings-${tab}`);
    for (const id of ["claude", "codex", "copilot", "cursor", "kilo", "openrouter", "litellm"]) expect(found).toContain(`mcp-approve-${id}`);
    for (const id of ["settings-save", "settings-discard", "settings-defaults", "skills-install", "project-notes", "search-chats", "live", "notifications", "sign-out", "check-update", "system-copy", "whats-new"]) expect(found).toContain(id);
  });

  it("lists the tabs of the open organization and one language entry per other language", () => {
    const found = ids();
    for (const tab of ["projects", "stats", "gate", "members", "repositories"]) expect(found).toContain(`org-tab-${tab}`);
    expect(found).toContain("language-pt-BR");
    expect(found).not.toContain("language-en");
  });

  it("hides save and discard while nothing changed and what the plan turns off", () => {
    const found = ids({ settingsDirty: false, rights: { features: new Set<string>(), locked: new Set<string>() } });
    expect(found).not.toContain("settings-save");
    expect(found).not.toContain("organizations");
    expect(found).not.toContain("project-notes");
  });

  it("has unique ids and a label for every command", () => {
    const list = paletteCommands(context());
    expect(new Set(list.map((command) => command.id)).size).toBe(list.length);
    expect(list.every((command) => command.label.trim().length > 0)).toBe(true);
  });
});
