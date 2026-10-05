import type { AgentSettings, CopilotOptions } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { updateOptions } from "@/modules/settings";
import { CheckList, FormField, OptionSelect } from "@/components/molecules";
import { MechanismsField } from "./MechanismsField";

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
      <MechanismsField agent="copilot" selected={options.mechanisms ?? []}
        onChange={(mechanisms) => set({ mechanisms, blockedTools: mechanisms.includes("shell") ? options.blockedTools.filter((tool) => tool !== "shell") : options.blockedTools })} />
      <FormField label={t("agent.tools")} hint={t("copilot.tools.hint")} wide>
        <CheckList id="copilot-tools" tone="danger" selected={options.blockedTools}
          onChange={(blockedTools) => set({ blockedTools, mechanisms: blockedTools.includes("shell") ? (options.mechanisms ?? []).filter((mechanism) => mechanism !== "shell") : options.mechanisms ?? [] })}
          items={TOOLS.map((value) => ({ value, label: t(`tool.${value}` as Key) }))} />
      </FormField>
    </div>
  );
}
