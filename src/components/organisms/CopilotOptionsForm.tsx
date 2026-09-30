import type { AgentSettings, CopilotOptions } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { updateOptions } from "@/modules/settings";
import { CheckList, FormField, OptionSelect, ToggleRow } from "@/components/molecules";

const ACCESS: CopilotOptions["toolAccess"][] = ["read", "edits", "all"];
const TOOLS = ["shell", "write", "shell(git push)", "shell(rm)"];

/** O que o Copilot pode executar sozinho. */
export function CopilotOptionsForm({ agent }: { agent: AgentSettings<"copilot"> }) {
  const t = useT();
  const options = agent.options;
  const set = (patch: Partial<CopilotOptions>) => updateOptions("copilot", patch);

  return (
    <div className="grid gap-5 sm:grid-cols-2">
      <FormField label={t("copilot.access")} htmlFor="copilot-access" hint={t(`copilot.access.${options.toolAccess}.hint`)} wide>
        <OptionSelect id="copilot-access" value={options.toolAccess} onChange={(toolAccess) => set({ toolAccess })}
          options={ACCESS.map((value) => ({ value, label: t(`copilot.access.${value}`) }))} />
      </FormField>
      <FormField label={t("agent.tools")} hint={t("copilot.tools.hint")} wide>
        <CheckList id="copilot-tools" tone="danger" selected={options.blockedTools} onChange={(blockedTools) => set({ blockedTools })}
          items={TOOLS.map((value) => ({ value, label: t(`tool.${value}` as Key) }))} />
      </FormField>
      <ToggleRow id="copilot-silent" label={t("copilot.silent")} hint={t("copilot.silent.hint")} checked={options.silent} onChange={(silent) => set({ silent })} className="sm:col-span-2" />
    </div>
  );
}
