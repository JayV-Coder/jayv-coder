import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));

const {
  defaultsOf, dependents, installMod, isModsDirty, isRequired, modAgentIds, missingRequirements, ownedAgents, restoreModDefaults, setModEnabled, statusOf, uninstallMod,
  unknownPoints, usable, useMods, withInstalled,
} = await import("./index");

type Manifest = import("@/modules/core").ModManifest;
type State = import("@/modules/core").InstalledMods;
const texts = (en: string): Record<string, string> => ({ en, "pt-BR": en });
const mod = (id: string, contributes: Manifest["contributes"], patch: Partial<Manifest> = {}): Manifest =>
  ({ id, version: "1.0.0", category: "project", plan: null, minApp: "0.95.0", preinstalled: false, title: texts(id), summary: texts(`${id}.`), contributes, ...patch });

const STATS = mod("stats", { features: ["stats"] }, { preinstalled: true });
const MCP = mod("mcp", { features: ["mcp"] }, { preinstalled: true });
const LOCAL = mod("local-ai", { agents: [{ kind: "api", protocol: "openai", baseUrl: "http://localhost:8080/v1", keyRequired: false }, { kind: "cli", command: "local", args: ["{prompt}"] }] }, { requires: ["mcp"] });
const CLAUDE = mod("claude", { llmEngines: ["claude"] }, { preinstalled: true });
const FUTURE = mod("future", { teleport: ["x"] });
const CATALOG = [CLAUDE, STATS, MCP, LOCAL, FUTURE];
const POINTS = ["llmEngines", "features", "agents"];

const state = (mods: Record<string, boolean>, removed: string[] = []): State => ({ mods: Object.fromEntries(Object.entries(mods).map(([id, enabled]) => [id, { enabled }])), removed });
const on = (value: State) => Object.fromEntries(Object.entries(value.mods).map(([id, item]) => [id, item.enabled]));

describe("the generic mod manager", () => {
  it("never turns off or uninstalls an essential mod", () => {
    const essential = mod("skills", { features: ["skills"] }, { preinstalled: true, required: true });
    const catalog = [...CATALOG, essential];
    const saved = state({ skills: true, stats: true });
    useMods.setState({ catalog, saved, draft: saved });
    expect(isRequired(catalog, "skills")).toBe(true);
    expect(isRequired(catalog, "stats")).toBe(false);
    setModEnabled("skills", false);
    uninstallMod("skills");
    setModEnabled("stats", false);
    expect(on(useMods.getState().draft)).toEqual({ skills: true, stats: false });
    useMods.setState({ catalog: CATALOG, saved: state({}), draft: state({}) });
  });

  it("knows when a mod runs on this app", () => {
    expect(usable(STATS, "0.95.0", POINTS)).toBe(true);
    expect(usable(STATS, "0.94.9", POINTS)).toBe(false);
    expect(usable(FUTURE, "9.0.0", POINTS)).toBe(false);
    expect(unknownPoints(FUTURE, POINTS)).toEqual(["teleport"]);
  });

  it("names the agents a mod brings like the core and the site", () => {
    expect(modAgentIds(LOCAL)).toEqual(["mod-local-ai", "mod-local-ai-2"]);
    expect(modAgentIds(CLAUDE)).toEqual(["claude"]);
    expect([...ownedAgents(CATALOG, state({ "local-ai": false }))]).toEqual(["mod-local-ai", "mod-local-ai-2"]);
    expect(ownedAgents(CATALOG, state({ stats: true })).size).toBe(0);
  });

  it("installing brings the requirements back, even a removed one", () => {
    const next = withInstalled(state({ stats: true }, ["mcp"]), CATALOG, "local-ai");
    expect(on(next)).toEqual({ stats: true, "local-ai": true, mcp: true });
    expect(next.removed).toEqual([]);
    expect(missingRequirements({ draft: state({ "local-ai": true, mcp: false }) }, LOCAL)).toEqual(["mcp"]);
    expect(dependents({ draft: state({ "local-ai": true, mcp: true }), catalog: CATALOG }, "mcp").map((item) => item.id)).toEqual(["local-ai"]);
  });

  it("the defaults reinstall the preinstalled mods and turn every installed one on, uninstalling nothing", () => {
    const next = defaultsOf(state({ stats: false, "local-ai": false }, ["claude"]), CATALOG);
    expect(on(next)).toEqual({ stats: true, "local-ai": true, claude: true, mcp: true });
    expect(next.removed).toEqual([]);
  });

  it("says how each installed mod is", () => {
    const view = (draft: State, catalog: Manifest[] | null = CATALOG, appVersion = "0.95.0") => ({ draft, catalog, appVersion, points: POINTS });
    expect(statusOf(view(state({ stats: true })), "stats")).toBe("active");
    expect(statusOf(view(state({ stats: false })), "stats")).toBe("off");
    expect(statusOf(view(state({ stats: true }), CATALOG, "0.94.0"), "stats")).toBe("needsApp");
    expect(statusOf(view(state({ future: true })), "future")).toBe("needsApp");
    expect(statusOf(view(state({ "local-ai": true, mcp: false })), "local-ai")).toBe("missing");
    expect(statusOf(view(state({ gone: true })), "gone")).toBe("absent");
    expect(statusOf(view(state({ gone: true }), null), "gone")).toBe("active");
  });

  it("changes the draft only, and the draft is dirty until it is saved or discarded", () => {
    const saved = state({ stats: true, mcp: true }, ["claude"]);
    useMods.setState({ catalog: CATALOG, saved, draft: saved });
    expect(isModsDirty(useMods.getState())).toBe(false);
    setModEnabled("stats", false);
    installMod("local-ai");
    uninstallMod("mcp");
    expect(on(useMods.getState().draft)).toEqual({ stats: false, "local-ai": true });
    expect(isModsDirty(useMods.getState())).toBe(true);
    setModEnabled("local-ai", true);
    expect(on(useMods.getState().draft)).toMatchObject({ mcp: true }, );
    restoreModDefaults();
    expect(on(useMods.getState().draft)).toEqual({ stats: true, mcp: true, "local-ai": true, claude: true });
    useMods.setState({ draft: saved });
    expect(isModsDirty(useMods.getState())).toBe(false);
  });
});
