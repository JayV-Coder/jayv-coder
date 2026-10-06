import type { Activity, AgentId, Aspect, EntryVerdict, ExitVerdict, RouteMode, TurnRoute, TurnStatus } from "@/modules/core";
import { t, type Key } from "@/modules/i18n";
import { AGENT_LABELS } from "@/modules/settings";
import { shownText, sourceLabel } from "./notice";

const GATE_WORDS: Record<string, Key> = { pass: "beat.gate.pass", ask: "beat.gate.ask", block: "beat.gate.block" };
const SUBTASK_WORDS: Record<string, Key> = { applied: "beat.subtask.applied", empty: "beat.subtask.empty", conflict: "beat.subtask.conflict", failed: "beat.subtask.failed" };
const agentLabel = (provider: unknown) => { const id = String(provider ?? ""); return id in AGENT_LABELS ? AGENT_LABELS[id as AgentId] : id; };

/** Uma etapa em uma linha. O evento cru é JSON; quem espera quer ler o que está
 * acontecendo, não o formato em que foi gravado. */
export function beatLine(kind: string, detail: Record<string, unknown>): string | null {
  const d = detail as Record<string, string | number | undefined>;
  switch (kind) {
    case "gate": {
      const word = GATE_WORDS[String(d.verdict)];
      return t("beat.gate", { verdict: word ? t(word) : String(d.verdict), score: d.score ?? "", demand: d.demand ?? "" });
    }
    case "read": return t("beat.read", { intent: word(INTENT_WORDS, d.intent), complexity: word(COMPLEXITY_WORDS, d.complexity), source: sourceLabel(String(d.source ?? "")) });
    case "context": return t("beat.context", { count: d.files ?? 0, tokens: d.tokens ?? 0 });
    case "route": {
      const said = routeLabel({ provider: String(d.provider ?? ""), model: String(d.model ?? ""), mode: (d.mode as RouteMode | undefined) ?? null, agent: (d.agent as string | undefined) ?? null });
      // O `reason` é diagnóstico do roteador, em inglês: não vai para a linha.
      return t("beat.routed", { route: said });
    }
    case "fallback": {
      const provider = String(d.provider ?? "");
      return t("beat.fallback", { agent: provider in AGENT_LABELS ? AGENT_LABELS[provider as AgentId] : provider, error: shownText(String(d.error ?? "")) });
    }
    case "review": {
      const provider = String(d.provider ?? "");
      return t("beat.review", { agent: provider in AGENT_LABELS ? AGENT_LABELS[provider as AgentId] : provider, model: String(d.model ?? ""), count: Number(d.files ?? 0) });
    }
    case "plan": {
      const provider = String(d.provider ?? "");
      return t("beat.plan", { agent: provider in AGENT_LABELS ? AGENT_LABELS[provider as AgentId] : provider, model: String(d.model ?? "") });
    }
    case "split": {
      const tasks = Array.isArray(detail.tasks) ? (detail.tasks as { title?: string; provider?: string }[]) : [];
      return t("beat.split", { count: tasks.length, parts: tasks.map((task) => `${task.title ?? ""} (${agentLabel(task.provider)})`).join(", ") });
    }
    case "subtask": {
      const key = SUBTASK_WORDS[String(d.outcome ?? "")] ?? "beat.subtask.failed";
      return t(key, { title: String(d.title ?? ""), agent: agentLabel(d.provider), patch: String(d.patch ?? "") });
    }
    case "running": return t("beat.running");
    case "agent": return agentLine(String(d.line ?? ""));
    case "done": return t("beat.done", { latency: d.latencyMs ?? 0, input: d.inputTokens ?? 0, output: d.outputTokens ?? 0 });
    case "failed": return t("beat.failed", { error: shownText(String(d.error ?? "")) });
    case "dismissed": return t("beat.dismissed", { prompt: d.prompt ?? "" });
    case "permission_denied": return t("beat.permissionDenied", { commands: Array.isArray(d.commands) ? d.commands.join(", ") : "" });
    default: return null;
  }
}

const INTENT_WORDS: Record<string, Key> = {
  analysis: "intent.analysis", code: "intent.code", frontend: "intent.frontend", general: "intent.general",
  refactor: "intent.refactor", review: "intent.review", security: "intent.security", test: "intent.test",
};
const COMPLEXITY_WORDS: Record<string, Key> = {
  trivial: "complexity.trivial", simple: "complexity.simple", medium: "complexity.medium", complex: "complexity.complex",
};

/** O identificador gravado dito no idioma de quem lê; um desconhecido sai como veio. */
function word(words: Record<string, Key>, value: unknown) {
  const id = String(value ?? "");
  return words[id] ? t(words[id]) : id;
}

/** O que o Codex chama cada item, quando ele não traz comando. */
const ITEM_WORDS: Record<string, Key> = {
  reasoning: "beat.agent.thinking", file_change: "beat.agent.editing", agent_message: "beat.agent.writing",
  todo_list: "beat.agent.planning", web_search: "beat.agent.searching",
};

/** A etapa de um agente de linha de comando. O núcleo grava o tipo do evento e
 * a ferramenta (`assistant: Read`, `system: init`, `item.started: ls`); aqui
 * isso vira uma frase. O nome da ferramenta ou o comando é dado do agente e
 * fica como veio. */
export function agentLine(line: string): string | null {
  if (!line) return null;
  const at = line.indexOf(": ");
  const kind = at < 0 ? line : line.slice(0, at);
  const detail = at < 0 ? "" : line.slice(at + 2).trim();
  if (kind === "system") return t("beat.agent.started");
  if (kind === "result") return t("beat.agent.finished");
  if (kind === "user") return t("beat.agent.toolResult");
  if (ITEM_WORDS[detail]) return t(ITEM_WORDS[detail]);
  // O Cursor manda a ferramenta como chave: `readToolCall`, `writeToolCall`.
  const tool = detail.replace(/ToolCall$/, "");
  return tool ? t("beat.agent.tool", { tool }) : t("beat.agent.working");
}

const MODE_WORDS: Record<RouteMode, Key> = { plan: "route.mode.plan", build: "route.mode.build" };
const ROLE_WORDS: Record<string, Key> = {
  developer: "route.agent.developer", frontend: "route.agent.frontend", security: "route.agent.security", reviewer: "route.agent.reviewer",
};

/** Quem atendeu, em uma linha: `Claude Code · sonnet · Build · desenvolvedor`. */
export function routeLabel(route: TurnRoute) {
  const provider = route.provider in AGENT_LABELS ? AGENT_LABELS[route.provider as AgentId] : route.provider;
  const parts = [provider, route.model];
  if (route.mode && MODE_WORDS[route.mode]) parts.push(t(MODE_WORDS[route.mode]));
  if (route.agent) parts.push(ROLE_WORDS[route.agent] ? t(ROLE_WORDS[route.agent]) : route.agent);
  return parts.filter(Boolean).join(" · ");
}

/** O que o modo quer dizer, para a dica do balão. */
export function routeHint(route: TurnRoute) {
  return route.mode && MODE_WORDS[route.mode] ? t(`${MODE_WORDS[route.mode]}.hint` as Key) : undefined;
}

export function beatLines(beats: Activity[]) {
  return beats.map((beat) => ({ seq: beat.seq, kind: beat.kind, line: beatLine(beat.kind, beat.detail ?? {}) }))
    .filter((beat): beat is { seq: number; kind: string; line: string } => Boolean(beat.line));
}

/** O que dizer enquanto se espera. Antes da primeira etapa não há o que contar,
 * e é aí que vale a frase de sempre; depois dela, a etapa mais recente diz mais
 * do que qualquer frase fixa. Quando a resposta começa a chegar, a faixa sai da
 * frente do texto e só avisa que ele está vindo. */
export function pendingWord(text: string, beats: Activity[]) {
  if (text) return t("pending.writing");
  return beatLines(beats).at(-1)?.line ?? t("pending.first");
}

/** Os dois portões falam por cores: é o mesmo semáforo que pinta o balão do
 * chat e o cartão da portaria. O nome vai como chave, para ser dito no idioma
 * de quem lê na hora de desenhar. */
export const ENTRY_VERDICTS: Record<EntryVerdict, { aspect: Aspect; label: Key }> = {
  pass: { aspect: "go", label: "verdict.entry.pass" },
  ask: { aspect: "ask", label: "verdict.entry.ask" },
  block: { aspect: "stop", label: "verdict.entry.block" },
};
export const EXIT_VERDICTS: Record<ExitVerdict, { aspect: Aspect; label: Key }> = {
  cleared: { aspect: "go", label: "verdict.exit.cleared" },
  held: { aspect: "stop", label: "verdict.exit.held" },
};

/** A luz de um balão. `aspect` nulo é a resposta sem cor: nada a segurou, e
 * nada a provou. */
export type MessageLight = { aspect: Aspect | null; label: Key };

/** O semáforo do balão. O pedido mostra o que o portão de entrada decidiu sobre
 * ele; a resposta, vermelho quando a portaria de saída segurou alguma coisa.
 * Resposta que passou pelas regras não é resposta verificada: o JayV não rodou
 * teste, build nem lint, então ela não fica verde — fica "não verificada", sem
 * cor. Um pedido barrado tinge os dois balões: quem respondeu foi a própria
 * portaria. */
export function messageLight(role: "user" | "assistant", turn: { status: TurnStatus; entry: EntryVerdict | null; exit: ExitVerdict | null }): MessageLight | null {
  if (role === "user") return turn.entry ? ENTRY_VERDICTS[turn.entry] : null;
  if (turn.status === "blocked") return { aspect: "stop", label: "verdict.blocked" };
  if (turn.status === "failed") return { aspect: "ask", label: "verdict.failed" };
  if (turn.status === "flying" || turn.status === "queued") return null;
  return turn.exit === "held" ? EXIT_VERDICTS.held : { aspect: null, label: "verdict.unverified" };
}
