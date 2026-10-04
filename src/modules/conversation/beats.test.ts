import { describe, expect, it, vi } from "vitest";

// O i18n lê o idioma guardado ao carregar; no teste não há navegador.
vi.hoisted(() => {
  const stored = new Map<string, string>();
  globalThis.localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => void stored.set(key, value) } as Storage;
});

const { agentLine, beatLine } = await import("./beats");

describe("beatLine", () => {
  it("says the intent and the complexity read by Jev as words, not identifiers", () => {
    const line = beatLine("read", { intent: "refactor", complexity: "medium", source: "jev" });
    expect(line).toContain("Refactor");
    expect(line).toContain("Medium");
    expect(line).not.toContain("refactor ·");
  });

  it("keeps the router diagnostic out of the route line", () => {
    const line = beatLine("route", { provider: "claude", model: "sonnet", reason: "code/medium wants a capable model" });
    expect(line).not.toContain("wants");
  });

  it("says which agent could not start before the next one takes over", () => {
    const line = beatLine("fallback", { provider: "codex", error: "Codex is not installed" });
    expect(line).toContain("Codex");
    expect(line).toContain("next agent");
  });

  it("names the agent that reviews the change and how many files it reads", () => {
    const line = beatLine("review", { provider: "cursor", model: "auto", files: 3 });
    expect(line).toContain("Cursor");
    expect(line).toContain("3 changed");
  });

  it("names the model that writes the plan before the build", () => {
    const line = beatLine("plan", { provider: "claude", model: "opus" });
    expect(line).toContain("Claude Code");
    expect(line).toContain("opus");
    expect(line).toContain("plan");
  });
});

describe("agentLine", () => {
  it("turns the raw agent events into sentences", () => {
    expect(agentLine("system: init")).toBe("The agent started its session");
    expect(agentLine("assistant: Read")).toBe("The agent is using Read");
    expect(agentLine("user")).toBe("The agent got a tool result back");
    expect(agentLine("result: success")).toBe("The agent finished");
    expect(agentLine("item.started: file_change")).toBe("The agent is changing files");
    expect(agentLine("item.started: cargo test")).toBe("The agent is using cargo test");
    expect(agentLine("tool_call: readToolCall")).toBe("The agent is using read");
    expect(agentLine("assistant")).toBe("The agent is working");
    expect(agentLine("")).toBeNull();
  });
});
