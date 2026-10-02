import { describe, expect, it } from "vitest";
import { emptyPolicy, policyLines, policyOk, policyPayload, policyProblems, storedPolicy } from "./policy";

describe("policyProblems", () => {
  it("a política vazia passa", () => {
    expect(policyOk(emptyPolicy())).toBe(true);
  });
  it("as mesmas recusas do banco", () => {
    expect(policyProblems({ ...emptyPolicy(), agents: [] })).toEqual(["agents"]);
    expect(policyProblems({ ...emptyPolicy(), blocked_models: ["opus"] })).toEqual(["models"]);
    expect(policyProblems({ ...emptyPolicy(), blocked_models: ["gemini/pro"] })).toEqual(["models"]);
    expect(policyOk({ ...emptyPolicy(), blocked_models: ["claude/claude-opus-4-1", "copilot/gpt-5@latest", "cursor/[auto]"] })).toBe(true);
    expect(policyProblems({ ...emptyPolicy(), deny: ["x".repeat(201)] })).toEqual(["patterns"]);
    expect(policyProblems({ ...emptyPolicy(), local_only: Array.from({ length: 51 }, (_, index) => `d${index}/**`) })).toEqual(["patterns"]);
  });
});

describe("policyPayload", () => {
  it("limpa as listas e põe os agentes na ordem do app", () => {
    const payload = policyPayload({ ...emptyPolicy(), agents: ["cursor", "claude"], deny: [" secrets/** ", "", "secrets/**"], blocked_models: ["claude/opus"] });
    expect(payload.agents).toEqual(["claude", "cursor"]);
    expect(payload.deny).toEqual(["secrets/**"]);
    expect(payload.blocked_models).toEqual(["claude/opus"]);
  });
  it("nulo continua sendo todos os agentes", () => {
    expect(policyPayload(emptyPolicy()).agents).toBeNull();
  });
});

describe("policyLines e storedPolicy", () => {
  it("uma linha por item", () => {
    expect(policyLines("a/**\n\n  b/**  \na/**")).toEqual(["a/**", "b/**"]);
  });
  it("a linha do banco vira o formulário, com o que faltar no padrão", () => {
    const stored = storedPolicy({ repository_id: null, agents: ["codex"], deny: ["*.pem"], min_shell: "deny", min_write: "talvez", safe_agents: true });
    expect(stored).toMatchObject({ repositoryId: null, agents: ["codex"], deny: ["*.pem"], blocked_models: [], min_shell: "deny", min_write: "allow", safe_agents: true, redact_secrets: false });
  });
});
