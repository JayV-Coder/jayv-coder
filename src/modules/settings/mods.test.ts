import { describe, expect, it, vi } from "vitest";

vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => undefined, removeItem: () => undefined });
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));
vi.mock("sonner", () => ({ toast: Object.assign(vi.fn(), { error: vi.fn(), success: vi.fn() }), Toaster: () => null }));

const { joinLine, lineProblem, modId, splitLine } = await import("./mods");
const { agentIds, agentLabel, cliAgentIds, createMod, EMPTY_MOD, problems, removeMod, useSettings } = await import("./store");

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

describe("created mods in the settings draft", () => {
  it("join the draft turned on, with their first model, after the built-in mods", () => {
    useSettings.setState({ agents: [], models: [] });
    const writer = createMod({ ...EMPTY_MOD, name: "Writer", command: "writer", args: "--model {model} {prompt}", edits: true, planArgs: "--dry-run {prompt}", model: "w-1" });
    const proxy = createMod({ ...EMPTY_MOD, name: "Alpha proxy", kind: "api", baseUrl: "http://localhost:8080/v1", apiKey: " sk-1 ", model: "a-1" });
    const { agents, models } = useSettings.getState();
    expect(agentIds(agents).slice(-2)).toEqual([proxy, writer]);
    expect(agentLabel(proxy, agents)).toBe("Alpha proxy");
    expect(agentLabel("mod-gone", agents)).toBe("gone");
    expect(agentLabel("claude", agents)).toBe("Claude Code");
    expect(cliAgentIds(agents)).toContain(writer);
    expect(cliAgentIds(agents)).not.toContain(proxy);
    const options = agents.find((agent) => agent.id === writer)?.options as { args: string[]; planArgs: string[] };
    expect(options.args).toEqual(["--model", "{model}", "{prompt}"]);
    expect(options.planArgs).toEqual(["--dry-run", "{prompt}"]);
    expect((agents.find((agent) => agent.id === proxy)?.options as { apiKey?: string }).apiKey).toBe("sk-1");
    expect(models.find((model) => model.agent === writer)?.capabilities).toContain("code");
    expect(models.find((model) => model.agent === proxy)?.capabilities).toEqual(["chat", "reasoning"]);
    expect(problems({ agents, models }, writer)).toEqual({});
    removeMod(writer);
    expect(useSettings.getState().agents.map((agent) => agent.id)).toEqual([proxy]);
    expect(useSettings.getState().models.every((model) => model.agent !== writer)).toBe(true);
    removeMod("claude");
    expect(useSettings.getState().agents).toHaveLength(1);
  });

  it("show what the core would refuse before saving", () => {
    useSettings.setState({ agents: [], models: [] });
    const writer = createMod({ ...EMPTY_MOD, name: "Writer", command: "my agent", args: "{effort}", edits: true });
    const proxy = createMod({ ...EMPTY_MOD, name: "Proxy", kind: "api", baseUrl: "", keyRequired: true });
    const state = useSettings.getState();
    expect(problems(state, writer)).toEqual({ command: "agent.command.hint", args: "mods.args.placeholder", planArgs: "mods.planArgs.required" });
    expect(problems(state, proxy)).toEqual({ baseUrl: "mods.baseUrl.required", apiKey: "gateway.apiKey.required" });
    useSettings.setState({ agents: [], models: [] });
  });
});
