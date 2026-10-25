import { useState } from "react";
import { ExternalLinkIcon } from "lucide-react";
import type { AgentSettings, CustomModOptions } from "@/modules/core";
import { useEnvironment } from "@/modules/environments";
import { reportError } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import { modsSitePath } from "@/modules/mods";
import { joinLine, splitLine, updateOptions } from "@/modules/settings";
import { openDashboard, SITE_URL } from "@/modules/site";
import { FormField, SegmentedControl, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** A linha de comando como a pessoa escreve: o texto fica como foi digitado
 * enquanto o campo está aberto, e os argumentos vão às opções a cada tecla. */
function LineField({ id, label, hint, error, value, placeholder, onChange }: {
  id: string; label: string; hint: string; error?: string; value: string[]; placeholder: string; onChange: (args: string[]) => void;
}) {
  const [text, setText] = useState(() => joinLine(value));
  // Os argumentos mudaram por fora (Descartar): o campo volta a eles.
  const shown = JSON.stringify(splitLine(text)) === JSON.stringify(value) ? text : joinLine(value);
  return (
    <FormField label={label} htmlFor={id} hint={hint} error={error}>
      <Input id={id} spellCheck={false} className="font-mono text-xs" value={shown} placeholder={placeholder} aria-invalid={error ? true : undefined}
        onChange={(event) => { setText(event.target.value); onChange(splitLine(event.target.value)); }} />
    </FormField>
  );
}

/** O que o mod criado é: o nome e, na linha de comando, as duas linhas e se
 * ele escreve no projeto; no ACP, a linha que abre o agente; na API, o
 * protocolo e se a chave é obrigatória. O
 * tipo não muda depois de criado (a aba mudaria inteira). Criar é no site; a
 * definição se edita lá ou aqui, e as duas mudanças vão pela sincronização. */
export function CustomModForm({ agent, problems }: { agent: AgentSettings; problems: Record<string, Key> }) {
  const t = useT();
  const environment = useEnvironment((state) => state.active);
  const options = agent.options as CustomModOptions;
  const set = (patch: Partial<CustomModOptions>) => updateOptions(agent.id, patch as never);
  const error = (field: string) => (problems[field] ? t(problems[field]) : undefined);

  return (
    <div className="grid gap-5">
      {SITE_URL && (
        <div className="flex flex-wrap items-center gap-3 rounded-md border border-border/60 px-3 py-2">
          <p className="min-w-0 flex-1 text-xs leading-snug text-muted-foreground">{t("mods.editOnSite.hint")}</p>
          <Button type="button" variant="outline" size="sm" onClick={() => void openDashboard(modsSitePath(environment)).catch(reportError)}>
            {t("mods.editOnSite")}<ExternalLinkIcon aria-hidden="true" />
          </Button>
        </div>
      )}
      <FormField label={t("mods.name")} htmlFor={`${agent.id}-name`} hint={t("mods.name.hint")} error={error("name")}>
        <Input id={`${agent.id}-name`} maxLength={40} value={options.name} aria-invalid={problems.name ? true : undefined} onChange={(event) => set({ name: event.target.value })} />
      </FormField>
      {options.kind === "acp" ? (
        <>
          <LineField id={`${agent.id}-args`} label={t("mods.args")} hint={t("mods.args.hint.acp")} error={error("args")} value={options.args} placeholder="acp" onChange={(args) => set({ args })} />
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label={t("mods.buildMode")} htmlFor={`${agent.id}-build-mode`} hint={t("mods.buildMode.hint")} error={error("buildMode")}>
              <Input id={`${agent.id}-build-mode`} spellCheck={false} className="font-mono text-xs" maxLength={64} placeholder="build" value={options.buildMode ?? ""} aria-invalid={problems.buildMode ? true : undefined} onChange={(event) => set({ buildMode: event.target.value.trim() })} />
            </FormField>
            <FormField label={t("mods.planMode")} htmlFor={`${agent.id}-plan-mode`} hint={t("mods.planMode.hint")} error={error("planMode")}>
              <Input id={`${agent.id}-plan-mode`} spellCheck={false} className="font-mono text-xs" maxLength={64} placeholder="plan" value={options.planMode ?? ""} aria-invalid={problems.planMode ? true : undefined} onChange={(event) => set({ planMode: event.target.value.trim() })} />
            </FormField>
          </div>
          <p className="text-sm leading-snug text-muted-foreground">{t("mods.acp.note")}</p>
        </>
      ) : options.kind === "cli" ? (
        <>
          <LineField id={`${agent.id}-args`} label={t("mods.args")} hint={t("mods.args.hint")} error={error("args")} value={options.args} placeholder="run --model {model} {prompt}" onChange={(args) => set({ args })} />
          <ToggleRow id={`${agent.id}-edits`} label={t("mods.edits")} hint={t("mods.edits.hint")} checked={options.edits} onChange={(edits) => set({ edits })} />
          <LineField id={`${agent.id}-plan-args`} label={t("mods.planArgs")} hint={t(options.edits ? "mods.planArgs.hint.required" : "mods.planArgs.hint")} error={error("planArgs")} value={options.planArgs} placeholder="run --read-only --model {model} {prompt}" onChange={(planArgs) => set({ planArgs })} />
        </>
      ) : (
        <>
          <FormField label={t("mods.protocol")} htmlFor={`${agent.id}-protocol`} hint={t("mods.protocol.hint")}>
            <SegmentedControl
              label={t("mods.protocol")}
              value={options.protocol}
              onChange={(protocol) => set({ protocol, ...(protocol !== "openai" ? { approveMcps: false } : {}) })}
              options={[
                { value: "openai", label: t("mods.protocol.openai") },
                { value: "anthropic", label: t("mods.protocol.anthropic") },
                { value: "responses", label: t("mods.protocol.responses") },
                { value: "manus", label: t("mods.protocol.manus") },
              ]}
            />
          </FormField>
          {options.protocol === "responses" && (
            <FormField label={t("mods.modelField")} htmlFor={`${agent.id}-model-field`} hint={t("mods.modelField.hint")} error={error("modelField")}>
              <Input id={`${agent.id}-model-field`} spellCheck={false} className="font-mono text-xs" maxLength={32} placeholder="model" value={options.modelField ?? ""} aria-invalid={problems.modelField ? true : undefined} onChange={(event) => set({ modelField: event.target.value.trim() })} />
            </FormField>
          )}
          <ToggleRow id={`${agent.id}-key-required`} label={t("mods.keyRequired")} hint={t("mods.keyRequired.hint")} checked={options.keyRequired} onChange={(keyRequired) => set({ keyRequired })} />
          <p className="text-sm leading-snug text-muted-foreground">{t("gateway.textOnly")}</p>
        </>
      )}
    </div>
  );
}
