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
vi.mock("@/modules/site", () => ({ SITE_URL: "https://site.test", dashboardUrl: (path = "") => `https://site.test/dashboard${path}`, openDashboard: vi.fn(async () => undefined), openSite: vi.fn(async () => undefined) }));

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
  environment: "personal",
  settingsDirty: true,
  rights: { features: null, locked: new Set<string>() },
  ...patch,
});
const ids = (patch = {}) => paletteCommands(context(patch)).map((command) => command.id);

describe("paletteCommands", () => {
  it("reaches every screen, every settings tab and the project actions", () => {
    const found = ids();
    for (const id of ["projects", "stats", "system", "settings", "profile", "plans", "gate", "chats", "new-chat", "new-project"]) expect(found).toContain(id);
    expect(found).not.toContain("organizations");
    for (const tab of ["app", "jev", "mcp", "skills", "claude", "codex", "copilot", "cursor"]) expect(found).toContain(`settings-${tab}`);
    for (const id of ["claude", "codex", "copilot", "cursor", "kilo", "openrouter", "litellm"]) expect(found).toContain(`mcp-approve-${id}`);
    for (const id of ["settings-save", "settings-discard", "settings-defaults", "skills-install", "project-notes", "search-chats", "live", "notifications", "sign-out", "check-update", "system-copy", "whats-new"]) expect(found).toContain(id);
  });

  it("lists the tabs of the open organization and one language entry per other language", () => {
    const found = ids();
    for (const tab of ["stats", "gate", "members", "repositories"]) expect(found).toContain(`org-tab-${tab}`);
    expect(found).toContain("language-pt-BR");
    expect(found).not.toContain("language-en");
  });

  it("hides save and discard while nothing changed and what the plan turns off", () => {
    const found = ids({ settingsDirty: false, rights: { features: new Set<string>(), locked: new Set<string>() } });
    expect(found).not.toContain("settings-save");
    expect(found).not.toContain("organizations");
    expect(found).not.toContain("project-notes");
  });

  it("shows only the organization inside its environment", () => {
    const organizations = [
      { id: "o1", name: "Acme", slug: "acme", role: "owner" as const, members: 1, repositories: 1 },
      { id: "o2", name: "Zeta", slug: "zeta", role: "member" as const, members: 3, repositories: 0 },
    ];
    const found = ids({ environment: "o1", organizations, openOrganizationId: null });
    for (const id of ["organizations", "system", "plans", "system-reload", "new-project", "org-o2", "org-tab-projects"]) expect(found).not.toContain(id);
    for (const id of ["projects", "stats", "settings", "profile", "org-o1", "org-tab-stats", "org-tab-members", "org-tab-repositories", "environment-personal", "environment-o2"]) expect(found).toContain(id);
  });

  it("reaches the Mods tab, Create mod and each created mod, as the plan allows", async () => {
    const { useSettings } = await import("@/modules/settings");
    const options = { name: "Local", kind: "api", args: [], planArgs: [], edits: false, protocol: "openai", baseUrl: "http://localhost:8080/v1", keyRequired: false, approveMcps: false, hasKey: false };
    useSettings.setState({ agents: [{ id: "mod-local", enabled: true, command: "", timeout: 300, options }] as never });
    const found = paletteCommands(context());
    for (const id of ["settings-mods", "mods-create", "mods-site", "settings-mod-local", "mcp-approve-mod-local", "gateway-check-mod-local"]) expect(found.map((command) => command.id)).toContain(id);
    expect(found.find((command) => command.id === "settings-mod-local")?.label).toBe("Settings › Local");
    const locked = ids({ rights: { features: new Set<string>(["mcp"]), locked: new Set<string>() } });
    for (const id of ["mods-create", "settings-mod-local", "mcp-approve-mod-local", "gateway-check-mod-local"]) expect(locked).not.toContain(id);
    expect(locked).toContain("settings-mods");
    expect(locked).toContain("mods-site");
    useSettings.setState({ agents: [] });
  });

  it("installs, turns on or off and uninstalls each mod of the catalog", async () => {
    const { useMods } = await import("@/modules/mods");
    const texts = (en: string) => ({ en, "pt-BR": en });
    const mod = (id: string, contributes: object, patch: object = {}) => ({ id, version: "1.0.0", category: "project", plan: null, minApp: "0.94.0", preinstalled: true, title: texts(id[0].toUpperCase() + id.slice(1)), summary: texts("."), contributes, ...patch });
    const state = { mods: { stats: { enabled: false }, mcp: { enabled: true } }, removed: [] };
    useMods.setState({
      appVersion: "0.95.0", points: ["llmEngines", "features", "agents"], saved: state, draft: state,
      catalog: [
        mod("stats", { features: ["stats"] }, { plan: "stats" }),
        mod("mcp", { features: ["mcp"] }, { plan: "mcp" }),
        mod("ollama", { agents: [{ kind: "api", protocol: "openai", baseUrl: "http://localhost:11434/v1", keyRequired: false }] }, { plan: "customMods" }),
        mod("future", { teleport: ["x"] }),
        mod("skills", { features: ["skills"] }, { plan: "skills", required: true }),
      ] as never,
    });
    const found = paletteCommands(context());
    const label = (id: string) => found.find((command) => command.id === id)?.label;
    expect(label("mod-toggle-stats")).toBe("Turn on mod: Stats");
    expect(label("mod-toggle-mcp")).toBe("Turn off mod: Mcp");
    expect(label("mod-uninstall-mcp")).toBe("Uninstall mod: Mcp");
    expect(label("mod-install-ollama")).toBe("Install mod: Ollama");
    expect(found.map((command) => command.id)).not.toContain("mod-install-future");
    for (const id of ["mod-install-skills", "mod-toggle-skills", "mod-uninstall-skills"]) expect(found.map((command) => command.id), "o essencial não muda").not.toContain(id);
    // Todo mod do catálogo tem a página dele no site, o essencial também.
    expect(label("mod-page-skills")).toBe("Mod page on the site: Skills");
    expect(label("mod-page-ollama")).toBe("Mod page on the site: Ollama");
    expect(label("site-mods")).toBe("Mod catalog on the site");
    const locked = ids({ rights: { features: new Set<string>(), locked: new Set<string>(["stats"]) } });
    expect(locked).not.toContain("mod-toggle-stats");
    expect(locked).not.toContain("mod-install-ollama");
    expect(locked).toContain("mod-toggle-mcp");
    const empty = { mods: {}, removed: [] };
    useMods.setState({ catalog: null, saved: empty, draft: empty });
    expect(ids().some((id) => id.startsWith("mod-toggle-"))).toBe(false);
  });

  it("has unique ids and a label for every command", () => {
    const list = paletteCommands(context());
    expect(new Set(list.map((command) => command.id)).size).toBe(list.length);
    expect(list.every((command) => command.label.trim().length > 0)).toBe(true);
  });
});
