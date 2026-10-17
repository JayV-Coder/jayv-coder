import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));

const { joinLine, lineProblem, modId, splitLine } = await import("./mods");
const { agentIds, agentLabel, cliAgentIds, problems, removeMod, useSettings } = await import("./store");
const { useEntitlements } = await import("@/modules/plans");

type Settings = import("@/modules/core").AgentSettings;
const WRITER: Settings = { id: "mod-writer", enabled: true, command: "writer", timeout: 300, options: { name: "Writer", kind: "cli", args: ["--model", "{model}", "{prompt}"], planArgs: ["--dry-run", "{prompt}"], edits: true, protocol: "openai", baseUrl: "", keyRequired: false, approveMcps: false, hasKey: false } };
const PROXY: Settings = { id: "mod-alpha-proxy", enabled: true, command: "", timeout: 300, options: { name: "Alpha proxy", kind: "api", args: [], planArgs: [], edits: false, protocol: "openai", baseUrl: "http://localhost:8080/v1", keyRequired: false, approveMcps: false, hasKey: false } };

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

  it("accept {prompt_file} alone, and only {model} in an ACP line", () => {
    expect(lineProblem(["--message-file", "{prompt_file}"])).toBeNull();
    expect(lineProblem(["--message-file={prompt_file}"])).toBe("mods.args.placeholder");
    expect(lineProblem(["acp", "--model", "{model}"], true)).toBeNull();
    expect(lineProblem(["acp", "{prompt}"], true)).toBe("mods.args.placeholder");
    expect(lineProblem(["acp", "{prompt_file}"], true)).toBe("mods.args.placeholder");
  });

  it("make an id from the name that never repeats", () => {
    expect(modId("Meu Modelo Local!", [])).toBe("mod-meu-modelo-local");
    expect(modId("Ação", ["mod-acao"])).toBe("mod-acao-2");
    expect(modId("***", [])).toBe("mod-mod");
    expect(modId("x".repeat(60), []).length).toBeLessThanOrEqual(36);
  });
});

describe("mods in the settings draft", () => {
  it("created mods come after the built-in ones, by name, and leave with their models", () => {
    const writer = "mod-writer" as const;
    const proxy = "mod-alpha-proxy" as const;
    const model = (agent: typeof writer | typeof proxy, name: string, capabilities: ("chat" | "code" | "reasoning" | "tools")[]) =>
      ({ uid: name, agent, model: name, enabled: true, capabilities, contextWindow: 128000, costClass: "medium" as const, speed: "medium" as const });
    useSettings.setState({ agents: [WRITER, PROXY], models: [model(writer, "w-1", ["chat", "code"]), model(proxy, "a-1", ["chat", "reasoning"])] });
    const { agents, models } = useSettings.getState();
    expect(agentIds(agents).slice(-2)).toEqual([proxy, writer]);
    expect(agentLabel(proxy, agents)).toBe("Alpha proxy");
    expect(agentLabel("mod-gone", agents)).toBe("gone");
    expect(agentLabel("claude", agents)).toBe("Claude Code");
    expect(cliAgentIds(agents)).toContain(writer);
    expect(cliAgentIds(agents)).not.toContain(proxy);
    expect(problems({ agents, models }, writer)).toEqual({});
    expect(problems({ agents, models }, proxy)).toEqual({});
    removeMod(writer);
    expect(useSettings.getState().agents.map((agent) => agent.id)).toEqual([proxy]);
    expect(useSettings.getState().models.every((item) => item.agent !== writer)).toBe(true);
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

  it("check the modes of an ACP mod and the model field of the Responses API", () => {
    const acp = "mod-acp" as const;
    const responses = "mod-responses" as const;
    const base = { args: [], planArgs: [], edits: false, protocol: "openai" as const, baseUrl: "", keyRequired: false, approveMcps: false, hasKey: false };
    useSettings.setState({
      models: [],
      agents: [
        { id: acp, enabled: true, command: "kimi", timeout: 300, options: { ...base, name: "Kimi", kind: "acp", args: ["acp", "{prompt}"], buildMode: "default", planMode: "plan mode" } },
        { id: responses, enabled: true, command: "", timeout: 300, options: { ...base, name: "Search", kind: "api", protocol: "responses", baseUrl: "https://api.example.com/v1", modelField: "Preset" } },
      ],
    });
    const state = useSettings.getState();
    expect(problems(state, acp)).toEqual({ args: "mods.args.placeholder", planMode: "mods.mode.invalid" });
    expect(problems(state, responses)).toEqual({ modelField: "mods.modelField.invalid" });
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
