import type { AgentId, Permission } from "@/modules/core";

/** A política de LLM como o banco a guarda (`organization_llm_policies`) e
 * como `set_llm_policy` a recebe. `agents` nulo é "todos os agentes". */
export interface LlmPolicy {
  agents: AgentId[] | null;
  blocked_models: string[];
  deny: string[];
  local_only: string[];
  safe_agents: boolean;
  redact_secrets: boolean;
  min_read: Permission;
  min_write: Permission;
  min_shell: Permission;
}

/** Uma política gravada: da organização (`repositoryId` nulo) ou de um
 * repositório dela. */
export interface StoredPolicy extends LlmPolicy { repositoryId: string | null; updatedAt: string }

export const POLICY_AGENTS: AgentId[] = ["claude", "codex", "copilot", "cursor"];
export const POLICY_RULES = ["read", "write", "shell"] as const;
export const PATTERNS_MAX = 50;
export const PATTERN_LENGTH_MAX = 200;
export const MODELS_MAX = 100;

export const emptyPolicy = (): LlmPolicy => ({
  agents: null, blocked_models: [], deny: [], local_only: [], safe_agents: false, redact_secrets: false,
  min_read: "allow", min_write: "allow", min_shell: "allow",
});

/** O mesmo `check` de `policy_models_ok`. */
const MODEL = /^(claude|codex|copilot|cursor)\/[A-Za-z0-9._:/@[\]-]{1,120}$/;

/** As linhas de um campo de texto, aparadas, sem vazias e sem repetição —
 * como o banco as grava. */
export function policyLines(text: string): string[] {
  return [...new Set(text.split("\n").map((line) => line.trim()).filter(Boolean))];
}

export type PolicyProblem = "agents" | "models" | "patterns";

/** O que o banco recusaria, antes de enviar. */
export function policyProblems(policy: LlmPolicy): PolicyProblem[] {
  const problems: PolicyProblem[] = [];
  if (policy.agents && (policy.agents.length === 0 || policy.agents.some((agent) => !POLICY_AGENTS.includes(agent)))) problems.push("agents");
  if (policy.blocked_models.length > MODELS_MAX || policy.blocked_models.some((model) => !MODEL.test(model))) problems.push("models");
  for (const patterns of [policy.deny, policy.local_only]) {
    if (patterns.length > PATTERNS_MAX || patterns.some((pattern) => pattern.length > PATTERN_LENGTH_MAX)) { problems.push("patterns"); break; }
  }
  return problems;
}

export const policyOk = (policy: LlmPolicy) => policyProblems(policy).length === 0;

/** A política pronta para a RPC: listas limpas e agentes na ordem do app. */
export function policyPayload(policy: LlmPolicy): LlmPolicy {
  const clean = (items: string[]) => policyLines(items.join("\n"));
  return {
    ...policy,
    agents: policy.agents ? POLICY_AGENTS.filter((agent) => policy.agents!.includes(agent)) : null,
    blocked_models: clean(policy.blocked_models),
    deny: clean(policy.deny),
    local_only: clean(policy.local_only),
  };
}

/** Uma linha de `organization_llm_policies` como o supabase-js a devolve. */
export function storedPolicy(row: Record<string, unknown>): StoredPolicy {
  const list = (value: unknown) => (Array.isArray(value) ? value.map(String) : []);
  const rule = (value: unknown): Permission => (value === "ask" || value === "deny" ? value : "allow");
  return {
    repositoryId: (row.repository_id as string | null) ?? null,
    updatedAt: String(row.updated_at ?? ""),
    agents: Array.isArray(row.agents) ? (row.agents as AgentId[]) : null,
    blocked_models: list(row.blocked_models),
    deny: list(row.deny),
    local_only: list(row.local_only),
    safe_agents: row.safe_agents === true,
    redact_secrets: row.redact_secrets === true,
    min_read: rule(row.min_read),
    min_write: rule(row.min_write),
    min_shell: rule(row.min_shell),
  };
}
