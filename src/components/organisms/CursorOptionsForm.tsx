import type { AgentSettings, CursorOptions } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { updateOptions } from "@/modules/settings";
import { FormField, OptionSelect, ToggleRow } from "@/components/molecules";

const SANDBOXES: CursorOptions["sandbox"][] = ["default", "enabled", "disabled"];

/** Onde o Cursor roda os comandos e o que ele faz sem pedir. */
export function CursorOptionsForm({ agent }: { agent: AgentSettings<"cursor"> }) {
  const t = useT();
  const options = agent.options;
  const set = (patch: Partial<CursorOptions>) => updateOptions("cursor", patch);

  return (
    <div className="grid gap-5 sm:grid-cols-2">
      <FormField label={t("cursor.sandbox")} htmlFor="cursor-sandbox" hint={t(`cursor.sandbox.${options.sandbox}.hint`)} wide>
        <OptionSelect id="cursor-sandbox" value={options.sandbox} onChange={(sandbox) => set({ sandbox })}
          options={SANDBOXES.map((value) => ({ value, label: t(`cursor.sandbox.${value}`) }))} />
      </FormField>
      <ToggleRow id="cursor-force" label={t("cursor.force")} hint={t("cursor.force.hint")} checked={options.force} onChange={(force) => set({ force })} />
      <ToggleRow id="cursor-mcps" label={t("cursor.approveMcps")} hint={t("cursor.approveMcps.hint")} checked={options.approveMcps} onChange={(approveMcps) => set({ approveMcps })} />
    </div>
  );
}
