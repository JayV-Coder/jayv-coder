import { useState } from "react";
import { Trash2Icon } from "lucide-react";
import { isApiAgent, isCustomMod, type AgentSettings, type CustomModOptions } from "@/modules/core";
import { useT, type Key } from "@/modules/i18n";
import { AGENT_LABELS, addModel, agentLabel, checkAgent, isAgentsDirty, refreshModels, removeMod, setSettingsTab, updateAgent, useSettings, type ModelDraft } from "@/modules/settings";
import { AgentIcon, EmptyText } from "@/components/atoms";
import { AgentProbeLine, ConfirmAction, FormField, OptionSelect, PAGE_SIZES, Pager, SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { ClaudeOptionsForm } from "./ClaudeOptionsForm";
import { CodexOptionsForm } from "./CodexOptionsForm";
import { CopilotOptionsForm } from "./CopilotOptionsForm";
import { CursorOptionsForm } from "./CursorOptionsForm";
import { CustomModForm } from "./CustomModForm";
import { GatewayConnection } from "./GatewayConnection";
import { KiloOptionsForm } from "./KiloOptionsForm";
import { McpApproveField } from "./McpApproveField";
import { ModelRow } from "./ModelRow";

export const AGENT_NAMES = AGENT_LABELS;
const TIMEOUTS = [60, 120, 300, 600, 900, 1800, 3600];

/** Tudo de um mod numa aba: se ele está ligado, como é chamado, o que pode
 * fazer e com quais modelos. O mod criado mostra também o que ele é (as linhas
 * de comando ou o protocolo) e pode ser apagado daqui. */
export function AgentPanel({ agent, models, problems }: { agent: AgentSettings; models: ModelDraft[]; problems: Record<string, Key> }) {
  const t = useT();
  const gateway = isApiAgent(agent);
  const custom = isCustomMod(agent.id);
  const options = agent.options as Partial<CustomModOptions>;
  const name = useSettings((state) => agentLabel(agent.id, state.agents));
  // O mod criado de linha de comando não sabe listar modelos: eles são os que
  // a pessoa escreve.
  const lists = !(custom && options.kind === "cli");
  // As ferramentas MCP chegam por linha de comando, arquivo, ambiente ou pelo
  // cliente MCP do app — este, só no protocolo da OpenAI.
  const mcp = !custom || (options.kind === "api" && options.protocol === "openai");
  const probe = useSettings((state) => state.probes[agent.id] ?? null);
  const [min, max] = useSettings((state) => state.timeoutRange);
  const refreshing = useSettings((state) => state.refreshing);
  const dirty = useSettings(isAgentsDirty);
  // Catálogos grandes travam a tela se todas as linhas forem montadas de uma vez.
  const [pageSize, setPageSize] = useState(10);
  const [wanted, setPage] = useState(0);
  const lastPage = Math.max(0, Math.ceil(models.length / pageSize) - 1);
  const page = Math.min(wanted, lastPage);
  const shown = models.slice(page * pageSize, (page + 1) * pageSize);
  const broken = models.findIndex((model) => problems[model.uid]);
  const brokenPage = broken < 0 ? page : Math.floor(broken / pageSize);
  const timeouts = [...new Set([...TIMEOUTS, agent.timeout])].filter((value) => value >= min && value <= max).sort((a, b) => a - b);
  const duration = (seconds: number) => (seconds % 60 === 0 ? t("agent.timeout.minutes", { count: seconds / 60 }) : t("agent.timeout.seconds", { count: seconds }));

  return (
    <div className="grid gap-5">
      <div className="flex flex-wrap items-center gap-4 rounded-xl border border-border/60 bg-card/40 px-6 py-5">
        <AgentIcon agent={agent.id} className="size-11 shrink-0" />
        <div className="min-w-0 flex-1">
          <h3 className="text-xl font-semibold break-words">{name}</h3>
          <p className="text-sm text-muted-foreground">{custom ? t(gateway ? "mods.tagline.api" : "mods.tagline.cli") : t(`agent.${agent.id}.tagline` as Key)}</p>
        </div>
        {custom && (
          <ConfirmAction title={t("mods.remove.title", { name })} description={t("mods.remove.description")} confirm={t("mods.remove")} onConfirm={() => { removeMod(agent.id); setSettingsTab("mods"); }}>
            <Button type="button" variant="outline" size="sm"><Trash2Icon aria-hidden="true" />{t("mods.remove")}</Button>
          </ConfirmAction>
        )}
        <label htmlFor={`${agent.id}-enabled`} className="flex items-center gap-3">
          <span className="grid text-end">
            <span className="text-sm font-medium">{t("agent.enabled")}</span>
            <span className="text-xs text-muted-foreground">{t("agent.enabled.hint")}</span>
          </span>
          <Switch id={`${agent.id}-enabled`} checked={agent.enabled} onCheckedChange={(enabled) => updateAgent(agent.id, { enabled })} />
        </label>
      </div>

      <SettingsSection title={t("agent.section.connection")} description={t(gateway ? "gateway.section.description" : "agent.section.connection.description")}>
        <div className="grid gap-5 sm:grid-cols-[1fr_220px]">
          {gateway ? <div className="sm:col-span-2"><GatewayConnection agent={agent} problems={problems} /></div> : <FormField label={t("agent.command")} htmlFor={`${agent.id}-command`} hint={t(custom ? "mods.command.hint" : "agent.command.locked")} error={problems.command && t(problems.command)}>
            <div className="flex gap-2">
              <Input
                id={`${agent.id}-command`}
                value={agent.command}
                disabled={!custom}
                onChange={(event) => updateAgent(agent.id, { command: event.target.value })}
                spellCheck={false}
                className="font-mono"
                aria-invalid={problems.command ? true : undefined}
              />
              <Button variant="outline" disabled={probe === "checking" || !!problems.command} onClick={() => void checkAgent(agent.id)}>{t("agent.check")}</Button>
            </div>
            <AgentProbeLine probe={probe} />
          </FormField>}
          <FormField label={t("agent.timeout")} htmlFor={`${agent.id}-timeout`} hint={t("agent.timeout.hint")}>
            <OptionSelect id={`${agent.id}-timeout`} value={String(agent.timeout)} onChange={(value) => updateAgent(agent.id, { timeout: Number(value) })}
              options={timeouts.map((value) => ({ value: String(value), label: duration(value) }))} />
          </FormField>
        </div>
      </SettingsSection>

      <SettingsSection title={t("agent.section.behavior")} description={t("agent.section.behavior.description")}>
        {agent.id === "claude" && <ClaudeOptionsForm agent={agent as AgentSettings<"claude">} models={models} problems={problems} />}
        {agent.id === "codex" && <CodexOptionsForm agent={agent as AgentSettings<"codex">} />}
        {agent.id === "copilot" && <CopilotOptionsForm agent={agent as AgentSettings<"copilot">} />}
        {agent.id === "cursor" && <CursorOptionsForm agent={agent as AgentSettings<"cursor">} />}
        {agent.id === "kilo" && <KiloOptionsForm agent={agent as AgentSettings<"kilo">} />}
        {custom && <CustomModForm agent={agent} problems={problems} />}
        {mcp && <McpApproveField agent={agent} />}
        {gateway && !custom && <p className="text-sm leading-snug text-muted-foreground">{t("gateway.textOnly")}</p>}
      </SettingsSection>

      <SettingsSection
        title={t("agent.section.models")}
        description={t("agent.section.models.description")}
        action={(
          <div className="flex gap-2">
            <Button
              variant="outline"
              size="sm"
              disabled={refreshing !== null || dirty || !lists}
              title={!lists ? t("mods.refresh.none") : dirty ? t("model.refresh.dirty") : t("model.refresh.hint")}
              onClick={() => void refreshModels(agent.id)}
            >
              {refreshing === agent.id ? t("model.refresh.running") : t("model.refresh")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => { addModel(agent.id); setPage(Math.floor(models.length / pageSize)); }}>{t("model.add")}</Button>
          </div>
        )}
      >
        {problems.models && <p role="alert" className="text-xs text-destructive">{t(problems.models)}</p>}
        {brokenPage !== page && (
          <p role="alert" className="text-xs text-destructive">
            {t("model.problemElsewhere")}{" "}
            <button type="button" className="font-medium underline underline-offset-2" onClick={() => setPage(brokenPage)}>{t("model.problemElsewhere.show")}</button>
          </p>
        )}
        {models.length === 0
          ? <EmptyText>{t("model.empty")}</EmptyText>
          : (
            <div className="grid gap-3">
              {shown.map((model) => <ModelRow key={model.uid} model={model} problem={problems[model.uid]} />)}
              {models.length > PAGE_SIZES[0] && (
                <Pager
                  id={`${agent.id}-page-size`}
                  page={page}
                  size={pageSize}
                  total={models.length}
                  onPage={setPage}
                  onSize={(size) => { setPageSize(size); setPage(Math.floor((page * pageSize) / size)); }}
                />
              )}
            </div>
          )}
      </SettingsSection>
    </div>
  );
}
