import { AGENT_LABELS } from "@/modules/settings";
import type { Key } from "@/modules/i18n";
import type { AgentId } from "@/modules/core";

/** Tokens, compactos a partir de dez mil: `12,4 mil`, `1,2 mi`. */
export function formatTokens(value: number, locale: string) {
  return new Intl.NumberFormat(locale, value >= 10_000 ? { notation: "compact", maximumFractionDigits: 1 } : {}).format(value);
}

/** O custo informado pelas ferramentas. Sem nenhum, "—": zero diria que
 * foi de graça. */
export function formatCost(value: number | null, locale: string) {
  if (value === null) return "—";
  return new Intl.NumberFormat(locale, { style: "currency", currency: "USD", maximumFractionDigits: value < 1 ? 4 : 2 }).format(value);
}

export function formatDuration(ms: number, locale: string) {
  if (ms < 1000) return new Intl.NumberFormat(locale, { style: "unit", unit: "millisecond" }).format(ms);
  const seconds = ms / 1000;
  if (seconds < 120) return new Intl.NumberFormat(locale, { style: "unit", unit: "second", maximumFractionDigits: 1 }).format(seconds);
  return new Intl.NumberFormat(locale, { style: "unit", unit: "minute", maximumFractionDigits: 0 }).format(seconds / 60);
}

export function formatPercent(value: number, locale: string) {
  return new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: value < 0.1 && value > 0 ? 1 : 0 }).format(value);
}

/** O nome de quem gastou. O núcleo grava `claude`, `codex`, `copilot`, `cursor`, `kilo`, `openrouter`, `litellm`,
 * `http:<provedor>` e `jev:<etapa>`; a etapa do Jev é traduzida. */
export function sourceLabel(source: string, t: (key: Key) => string) {
  if (source in AGENT_LABELS) return AGENT_LABELS[source as AgentId];
  if (source.startsWith("http:")) return source.slice(5) in AGENT_LABELS ? AGENT_LABELS[source.slice(5) as AgentId] : source.slice(5);
  if (source.startsWith("jev:")) {
    const stage = source.slice(4);
    const key = `usage.jev.stage.${stage}` as Key;
    const said = t(key);
    return said === key ? `Jev · ${stage}` : said;
  }
  return source;
}

/** O nome do agente de um limite: os CLIs e o próprio Jev. */
export function agentLabel(agent: string) {
  if (agent in AGENT_LABELS) return AGENT_LABELS[agent as AgentId];
  return agent === "jev" ? "Jev" : agent;
}

/** A janela de um limite: sessão de 5 horas, semana, mês, dia do Jev, ou a
 * semana de uma família de modelos (`week:sonnet_only`). */
export function windowLabel(window: string, t: (key: Key, params?: Record<string, string>) => string) {
  if (window.startsWith("week:")) return t("usage.window.weekOf", { name: window.slice(5).replace(/_/g, " ") });
  const key = `usage.window.${window}` as Key;
  const said = t(key);
  return said === key ? window : said;
}

/** A hora em que o limite renova. O Codex e o Jev mandam um instante; o
 * Claude, a frase no fuso da conta, que fica como veio. */
export function formatReset(value: string, locale: string) {
  // Só o instante ISO vira data; a frase do Claude o motor do WebView
  // leria com outro ano e outro fuso.
  if (!/^\d{4}-\d{2}-\d{2}T/.test(value)) return value;
  const at = new Date(value);
  if (Number.isNaN(at.getTime())) return value;
  return new Intl.DateTimeFormat(locale, { weekday: "short", day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" }).format(at);
}

// As cores vêm dos tokens do tema: o mesmo agente muda de tom com o tema,
// não de cor.
const PALETTE = [1, 2, 3, 4, 5, 6, 7, 8].map((index) => `var(--chart-${index})`);

/** Uma cor fixa por fonte: o mesmo agente tem a mesma cor em todo gráfico. */
export function sourceColor(source: string) {
  if (source.startsWith("jev:")) return PALETTE[PALETTE.length - 1];
  let hash = 0;
  for (const char of source) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return PALETTE[hash % (PALETTE.length - 1)];
}
