import type { AgentProbe, SystemStatus } from "@/modules/core";
import type { Link } from "@/modules/core/bridge";

/** Como cada peça do sistema está: em dia, pedindo atenção, quebrada ou fora
 * de uso. É a cor da bolinha na página Sistema. */
export type Health = "ok" | "warn" | "fail" | "off";

/** A conexão com o servidor: fora do ar e sessão vencida pedem atenção; uma
 * escrita recusada é falha, porque não se resolve sozinha. */
export function connectionHealth(link: Link, pending: number, refused: number): Health {
  if (refused > 0) return "fail";
  if (link === "signedOut") return "off";
  if (link !== "online") return "warn";
  return pending > 0 ? "warn" : "ok";
}

/** O agente: achado e respondendo, achado e mudo, não achado ou sem comando
 * configurado (`null`). */
export function agentHealth(probe: AgentProbe | null): Health {
  if (!probe) return "off";
  if (!probe.path) return "fail";
  return probe.version ? "ok" : "warn";
}

/** As tabelas do banco local, da que tem mais linhas para a que tem menos, e
 * o total. */
export function tableSummary(tables: SystemStatus["tables"]) {
  const sorted = [...tables].sort((a, b) => b.rows - a.rows || a.name.localeCompare(b.name));
  return { sorted, total: tables.reduce((sum, table) => sum + table.rows, 0), largest: sorted[0]?.rows ?? 0 };
}

export interface DiagnosticInput {
  status: SystemStatus;
  link: Link;
  pending: number;
  refused: number;
  agents: { label: string; probe: AgentProbe | null }[];
  platform: string;
  language: string;
  at: Date;
}

/** O texto que "Copiar diagnóstico" põe na área de transferência, para colar
 * num pedido de ajuda. Leva versão, sistema, conexão, agentes, números e
 * tabelas. Não leva conteúdo de chat, chave ou token: só caminhos e contagens. */
export function diagnosticReport(input: DiagnosticInput): string {
  const { status } = input;
  const agent = ({ label, probe }: DiagnosticInput["agents"][number]) =>
    `- ${label}: ${!probe ? "not configured" : !probe.path ? "not found" : `${probe.path}${probe.version ? ` (${probe.version})` : " (no answer)"}`}`;
  return [
    `JayV ${status.version}`,
    `Generated: ${input.at.toISOString()}`,
    `Platform: ${input.platform}`,
    `Language: ${input.language}`,
    `Connection: ${input.link}, ${input.pending} pending, ${input.refused} refused`,
    "",
    "Agents:",
    ...input.agents.map(agent),
    "",
    `Providers: ${status.providers}`,
    `Models: ${status.models}`,
    `Indexed files: ${status.indexed_files}`,
    `Cache entries: ${status.cache_entries}`,
    `Session messages: ${status.session_messages}`,
    `Router records: ${status.performance_records}`,
    "",
    `Config: ${status.config_path}`,
    `Database: ${status.database_path}`,
    ...tableSummary(status.tables).sorted.map((table) => `- ${table.name}: ${table.rows}`),
  ].join("\n");
}
