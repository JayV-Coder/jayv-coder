import type { AgentSettings, CodexOptions } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { updateOptions } from "@/modules/settings";
import { FormField, OptionSelect, ToggleRow } from "@/components/molecules";
import { MechanismsField } from "./MechanismsField";

const SANDBOXES: CodexOptions["sandbox"][] = ["read-only", "workspace-write", "danger-full-access"];
const EFFORTS: CodexOptions["reasoningEffort"][] = ["auto", "low", "medium", "high"];

/** Onde o Codex pode mexer e quanto ele pensa. */
export function CodexOptionsForm({ agent }: { agent: AgentSettings<"codex"> }) {
  const t = useT();
  const options = agent.options;
  const set = (patch: Partial<CodexOptions>) => updateOptions("codex", patch);
  // Sem sandbox a rede já está aberta; nos outros dois, a escolha vale quando
  // o Codex escreve — e no `read-only` o modo Desenvolvimento o faz escrever.
  const open = options.sandbox === "danger-full-access";

  return (
    <div className="grid gap-5 sm:grid-cols-2">
      <FormField label={t("codex.sandbox")} htmlFor="codex-sandbox" hint={t(`codex.sandbox.${options.sandbox}.hint`)} wide>
        <OptionSelect id="codex-sandbox" value={options.sandbox} onChange={(sandbox) => set({ sandbox, networkAccess: sandbox !== "danger-full-access" && options.networkAccess })}
          options={SANDBOXES.map((value) => ({ value, label: t(`codex.sandbox.${value}`) }))} />
      </FormField>
      <FormField label={t("agent.effort")} htmlFor="codex-effort" hint={t("agent.effort.hint.auto")} wide>
        <OptionSelect id="codex-effort" value={options.reasoningEffort} onChange={(reasoningEffort) => set({ reasoningEffort })}
          options={EFFORTS.map((value) => ({ value, label: t(`effort.${value}`) }))} />
      </FormField>
      <MechanismsField agent="codex" selected={options.mechanisms ?? []} onChange={(mechanisms) => set({ mechanisms })} />
      <ToggleRow id="codex-network" label={t("codex.network")} hint={t(open ? "codex.network.open" : "codex.network.hint")} checked={open || options.networkAccess} disabled={open} onChange={(networkAccess) => set({ networkAccess })} />
    </div>
  );
}
