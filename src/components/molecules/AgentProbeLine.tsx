import type { ProbeState } from "@/modules/settings";
import { useT } from "@/modules/i18n";
import { cn } from "@/lib/utils";

/** Se o executável do agente está onde a configuração diz. */
export function AgentProbeLine({ probe }: { probe: ProbeState }) {
  const t = useT();
  if (probe === null) return null;
  if (probe === "checking") return <p className="text-[11.5px] text-muted-foreground">{t("agent.checking")}</p>;
  const found = probe.path !== null;
  return (
    <p className={cn("flex flex-wrap items-center gap-x-2 text-[11.5px]", found ? "text-emerald-400" : "text-amber-400")} role="status">
      <span aria-hidden="true" className={cn("size-1.5 rounded-full", found ? "bg-emerald-400" : "bg-amber-400")} />
      {found ? t("agent.found", { path: probe.path! }) : t("agent.missing")}
      {found && probe.version && <span className="font-mono text-muted-foreground">{probe.version}</span>}
    </p>
  );
}
