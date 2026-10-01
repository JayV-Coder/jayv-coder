import type { Activity, Aspect, EntryVerdict, ExitVerdict, TurnStatus } from "@/modules/core";
import { t, type Key } from "@/modules/i18n";
import { shownText, sourceLabel } from "./notice";

const GATE_WORDS: Record<string, Key> = { pass: "beat.gate.pass", ask: "beat.gate.ask", block: "beat.gate.block" };

/** Uma etapa em uma linha. O evento cru é JSON; quem espera quer ler o que está
 * acontecendo, não o formato em que foi gravado. */
export function beatLine(kind: string, detail: Record<string, unknown>): string | null {
  const d = detail as Record<string, string | number | undefined>;
  switch (kind) {
    case "gate": {
      const word = GATE_WORDS[String(d.verdict)];
      return t("beat.gate", { verdict: word ? t(word) : String(d.verdict), score: d.score ?? "", demand: d.demand ?? "" });
    }
    case "read": return t("beat.read", { intent: d.intent ?? "", complexity: d.complexity ?? "", source: sourceLabel(String(d.source ?? "")) });
    case "context": return t("beat.context", { count: d.files ?? 0, tokens: d.tokens ?? 0 });
    case "route": return `${t("beat.route", { provider: d.provider ?? "", model: d.model ?? "" })}${d.reason ? ` — ${d.reason}` : ""}`;
    case "running": return t("beat.running");
    case "agent": return String(d.line ?? "");
    case "done": return t("beat.done", { latency: d.latencyMs ?? 0, input: d.inputTokens ?? 0, output: d.outputTokens ?? 0 });
    case "failed": return t("beat.failed", { error: shownText(String(d.error ?? "")) });
    case "dismissed": return t("beat.dismissed", { prompt: d.prompt ?? "" });
    default: return null;
  }
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

/** O semáforo do balão. O pedido mostra o que o portão de entrada decidiu sobre
 * ele; a resposta, o que o portão de saída viu nela — vermelho quando alguma
 * coisa foi segurada, verde quando nada foi. Um pedido barrado tinge os dois
 * balões: quem respondeu foi a própria portaria. */
export function messageLight(role: "user" | "assistant", turn: { status: TurnStatus; entry: EntryVerdict | null; exit: ExitVerdict | null }): { aspect: Aspect; label: Key } | null {
  if (role === "user") return turn.entry ? ENTRY_VERDICTS[turn.entry] : null;
  if (turn.status === "blocked") return { aspect: "stop", label: "verdict.blocked" };
  if (turn.status === "failed") return { aspect: "ask", label: "verdict.failed" };
  if (turn.status === "flying" || turn.status === "queued") return null;
  return turn.exit ? EXIT_VERDICTS[turn.exit] : { aspect: "go", label: "verdict.nothing" };
}
