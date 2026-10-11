import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));

const { joinLine, lineProblem, modId, splitLine } = await import("./mods");
const { agentIds, agentLabel, cliAgentIds, installTemplate, problems, removeMod, useSettings } = await import("./store");
const { useEntitlements } = await import("@/modules/plans");

type Manifest = import("@/modules/core").ModManifest;
const texts = (en: string, pt = en): Record<string, string> => ({ en, "pt-BR": pt });
const template = (id: string, name: string, definition: Manifest["template"]): Manifest =>
  ({ id, kind: "template", category: "agents", plan: "customMods", minApp: "0.94.0", title: texts(name), summary: texts(name), template: definition });
const WRITER = template("writer", "Writer", { kind: "cli", command: "writer", args: ["--model", "{model}", "{prompt}"], planArgs: ["--dry-run", "{prompt}"], edits: true, models: ["w-1"] });
const PROXY = template("alpha-proxy", "Alpha proxy", { kind: "api", protocol: "openai", baseUrl: "http://localhost:8080/v1", keyRequired: false, models: ["a-1"] });

describe("command lines of a created mod", () => {
  it("split like a shell, without expanding, and join back", () => {
    const args = splitLine(`run --model {model} --system "be brief" '{prompt}'`);
    expect(args).toEqual(["run", "--model", "{model}", "--system", "be brief", "{prompt}"]);
    expect(splitLine(joinLine(args))).toEqual(args);
    expect(splitLine(joinLine(["say \"hi\"", ""]))).toEqual(["say \"hi\"", ""]);
    expect(splitLine("   ")).toEqual([]);
  });

  it("accept only {model} anywhere and {prompt} alone", () => {
    expect(lineProblem(["--model={model}", "{prompt}"])).toBeNull();
    expect(lineProblem(["--resume", "{resume}"])).toBe("mods.args.placeholder");
    expect(lineProblem(["--prompt={prompt}"])).toBe("mods.args.placeholder");
    expect(lineProblem(["--json", "{\"a\":1}"])).toBeNull();
    expect(lineProblem(["a\nb"])).toBe("mods.args.invalid");
    expect(lineProblem([""])).toBe("mods.args.invalid");
  });

  it("make an id from the name that never repeats", () => {
    expect(modId("Meu Modelo Local!", [])).toBe("mod-meu-modelo-local");
    expect(modId("Ação", ["mod-acao"])).toBe("mod-acao-2");
    expect(modId("***", [])).toBe("mod-mod");
    expect(modId("x".repeat(60), []).length).toBeLessThanOrEqual(36);
  });
});

describe("mods in the settings draft", () => {
  it("a template from the catalog joins the draft turned on, with its models, after the built-in mods", () => {
    useSettings.setState({ agents: [], models: [] });
    const writer = installTemplate(WRITER, "en");
    const proxy = installTemplate(PROXY, "pt-BR");
    expect([writer, proxy]).toEqual(["mod-writer", "mod-alpha-proxy"]);
    expect(installTemplate(WRITER, "en")).toBeNull();
    const { agents, models } = useSettings.getState();
    expect(agentIds(agents).slice(-2)).toEqual([proxy, writer]);
    expect(agentLabel(proxy!, agents)).toBe("Alpha proxy");
    expect(agentLabel("mod-gone", agents)).toBe("gone");
    expect(agentLabel("claude", agents)).toBe("Claude Code");
    expect(cliAgentIds(agents)).toContain(writer);
    expect(cliAgentIds(agents)).not.toContain(proxy);
    const options = agents.find((agent) => agent.id === writer)?.options as { args: string[]; planArgs: string[]; edits: boolean };
    expect(options).toMatchObject({ args: ["--model", "{model}", "{prompt}"], planArgs: ["--dry-run", "{prompt}"], edits: true });
    expect(models.find((model) => model.agent === writer)?.capabilities).toContain("code");
    expect(models.find((model) => model.agent === proxy)?.capabilities).toEqual(["chat", "reasoning"]);
    expect(problems({ agents, models }, writer!)).toEqual({});
    expect(problems({ agents, models }, proxy!)).toEqual({});
    removeMod(writer!);
    expect(useSettings.getState().agents.map((agent) => agent.id)).toEqual([proxy]);
    expect(useSettings.getState().models.every((model) => model.agent !== writer)).toBe(true);
    removeMod("claude");
    expect(useSettings.getState().agents).toHaveLength(1);
  });

  it("show what the core would refuse before saving", () => {
    const writer = "mod-writer" as const;
    const proxy = "mod-proxy" as const;
    useSettings.setState({
      models: [],
      agents: [
        { id: writer, enabled: true, command: "my agent", timeout: 300, options: { name: "Writer", kind: "cli", args: ["{effort}"], planArgs: [], edits: true, protocol: "openai", baseUrl: "", keyRequired: false, approveMcps: false, hasKey: false } },
        { id: proxy, enabled: true, command: "", timeout: 300, options: { name: "Proxy", kind: "api", args: [], planArgs: [], edits: false, protocol: "openai", baseUrl: "", keyRequired: true, approveMcps: false, hasKey: false } },
      ],
    });
    const state = useSettings.getState();
    expect(problems(state, writer)).toEqual({ command: "agent.command.hint", args: "mods.args.placeholder", planArgs: "mods.planArgs.required" });
    expect(problems(state, proxy)).toEqual({ baseUrl: "mods.baseUrl.required", apiKey: "gateway.apiKey.required" });
    useSettings.setState({ agents: [], models: [] });
  });

  it("a built-in engine out of the catalog of mods has no tab", () => {
    useEntitlements.setState({ disabled: new Set(["cursor", "stats"]) });
    expect(agentIds([])).not.toContain("cursor");
    expect(agentIds([])).toContain("claude");
    useEntitlements.setState({ disabled: new Set() });
    expect(agentIds([])).toContain("cursor");
  });
});
