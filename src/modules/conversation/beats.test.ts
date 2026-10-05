import { describe, expect, it, vi } from "vitest";

// O i18n lê o idioma guardado ao carregar; no teste não há navegador.
vi.hoisted(() => {
  const stored = new Map<string, string>();
  globalThis.localStorage = { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => void stored.set(key, value) } as Storage;
});

const { agentLine, beatLine, messageLight } = await import("./beats");

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

  it("lists the parts of a split request and who does each", () => {
    const line = beatLine("split", { tasks: [{ title: "API", provider: "codex", model: "gpt-5.5" }, { title: "Screen", provider: "claude", model: "sonnet" }] });
    expect(line).toContain("2 parts");
    expect(line).toContain("API (Codex)");
    expect(line).toContain("Screen (Claude Code)");
  });

  it("says where the patch of a part that did not fit was kept", () => {
    const line = beatLine("subtask", { index: 1, title: "Screen", provider: "cursor", outcome: "conflict", patch: "/repo/.git/jayv-patches/c-2.patch" });
    expect(line).toContain("Screen");
    expect(line).toContain("/repo/.git/jayv-patches/c-2.patch");
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

describe("messageLight", () => {
  const answered = { status: "answered" as const, entry: "pass" as const, exit: null };

  it("never paints an answer green: passing the house rules is not being verified", () => {
    expect(messageLight("assistant", answered)).toEqual({ aspect: null, label: "verdict.unverified" });
    expect(messageLight("assistant", { ...answered, exit: "cleared" })).toEqual({ aspect: null, label: "verdict.unverified" });
  });

  it("keeps red for what the exit gate held and for a blocked request", () => {
    expect(messageLight("assistant", { ...answered, exit: "held" })?.aspect).toBe("stop");
    expect(messageLight("assistant", { ...answered, status: "blocked" })?.aspect).toBe("stop");
  });

  it("still shows the entry verdict on the request", () => {
    expect(messageLight("user", answered)?.aspect).toBe("go");
  });
});
