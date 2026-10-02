import type { ProbeState } from "@/modules/settings";
import { useT } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** Se o executável do agente está onde a configuração diz. */
export function AgentProbeLine({ probe }: { probe: ProbeState }) {
  const t = useT();
  if (probe === null) return null;
  if (probe === "checking") return <p className="text-xs text-muted-foreground">{t("agent.checking")}</p>;
  // Achado e respondendo; achado mas mudo (o script do npm sem Node, por
  // exemplo); ou não achado.
  const found = probe.path !== null;
  const healthy = found && !!probe.version;
  return (
    <p className={cn("flex flex-wrap items-center gap-x-2 text-xs", healthy ? "text-success" : "text-warning")} role="status">
      <span aria-hidden="true" className={cn("size-1.5 rounded-full", healthy ? "bg-success" : "bg-warning")} />
      {healthy ? t("agent.found", { path: probe.path! }) : found ? t("agent.silent", { path: probe.path! }) : t("agent.missing")}
      {healthy && <span className="font-mono text-muted-foreground">{probe.version}</span>}
    </p>
  );
}
