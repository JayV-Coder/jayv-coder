import { PROBE_EVERY_MS, type ProbeState } from "@/modules/settings";
import { useT } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** Se o executável do agente está onde a configuração diz. */
export function AgentProbeLine({ probe }: { probe: ProbeState }) {
  const t = useT();
  if (probe === null) return null;
  if (probe === "checking") return <p className="text-[11.5px] text-muted-foreground">{t("agent.checking")}</p>;
  // Achado e respondendo; achado mas mudo (o script do npm sem Node, por
  // exemplo); ou não achado.
  const found = probe.path !== null;
  const healthy = found && !!probe.version;
  return (
    <p className={cn("flex flex-wrap items-center gap-x-2 text-[11.5px]", healthy ? "text-emerald-400" : "text-amber-400")} role="status">
      <span aria-hidden="true" className={cn("size-1.5 rounded-full", healthy ? "bg-emerald-400" : "bg-amber-400")} />
      {healthy ? t("agent.found", { path: probe.path! }) : found ? t("agent.silent", { path: probe.path! }) : t("agent.missing")}
      {healthy && <span className="font-mono text-muted-foreground">{probe.version}</span>}
      <span className="text-muted-foreground/70">· {t("agent.watching", { seconds: PROBE_EVERY_MS / 1000 })}</span>
    </p>
  );
}
