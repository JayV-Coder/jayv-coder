import type { AgentSettings } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { updateOptions } from "@/modules/settings";
import { ToggleRow } from "@/components/molecules";

/** O que o Kilo Code faz sem pedir. No modo desenvolvimento ele sempre
 * aprova sozinho; no planejamento, nunca. */
export function KiloOptionsForm({ agent }: { agent: AgentSettings<"kilo"> }) {
  const t = useT();
  return (
    <div className="grid gap-5">
      <ToggleRow id="kilo-auto" label={t("kilo.auto")} hint={t("kilo.auto.hint")} checked={agent.options.auto} onChange={(auto) => updateOptions("kilo", { auto })} />
      <p className="text-xs leading-snug text-muted-foreground">{t("kilo.mechanisms.none")}</p>
    </div>
  );
}
