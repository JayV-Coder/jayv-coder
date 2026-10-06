import { useConnection } from "@/modules/connection";
import { notify } from "@/modules/feedback";
import { translate, useI18n } from "@/modules/i18n";
import { AGENT_LABELS, AGENTS, checkAllAgents, useSettings, type ProbeState } from "@/modules/settings";
import { diagnosticReport } from "./report";
import { loadStatus, useSystem } from "./store";

/** Confere de novo o núcleo e os agentes, como o botão "Conferir de novo". */
export function reloadSystem() {
  void loadStatus();
  checkAllAgents();
}

/** Junta o diagnóstico num texto e copia, como o botão "Copiar diagnóstico";
 * sem o retrato do sistema carregado, carrega antes. */
export async function copySystemReport() {
  if (!useSystem.getState().status) await loadStatus();
  const status = useSystem.getState().status;
  if (!status) return;
  const { link, pending, refusals } = useConnection.getState();
  const { agents, probes } = useSettings.getState();
  const locale = useI18n.getState().locale;
  const configured = Object.fromEntries(agents.map((agent) => [agent.id, agent.command.trim() !== ""])) as Partial<Record<string, boolean>>;
  const probeOf = (probe: ProbeState) => (probe === "checking" ? null : probe);
  const text = diagnosticReport({
    status, link, pending, refused: refusals.unplaced, platform: navigator.userAgent, language: locale, at: new Date(),
    agents: AGENTS.map((id) => ({ label: AGENT_LABELS[id], probe: configured[id] === false ? null : probeOf(probes[id]) })),
  });
  await navigator.clipboard.writeText(text);
  notify(translate(locale, "system.copied"));
}
