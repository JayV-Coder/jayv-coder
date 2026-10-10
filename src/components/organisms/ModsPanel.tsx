import { PlusIcon, SettingsIcon, Trash2Icon } from "lucide-react";
import { isApiAgent, isCustomMod, type AgentSettings } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { allows, settingsTabFeature, useEntitlements, useFeature } from "@/modules/plans";
import { agentLabel, MAX_MODS, removeMod, setSettingsTab, updateAgent, useAgentIds, useSettings } from "@/modules/settings";
import { AgentIcon } from "@/components/atoms";
import { ConfirmAction, SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { ModCreateDialog } from "./ModCreateDialog";

/** Configurações › Mods: toda integração com modelos de LLM é um mod. Os que
 * vêm com o app (Claude Code, Codex, Copilot, Cursor, Kilo Code, OpenRouter e
 * LiteLLM) e os criados aqui ficam na mesma lista, cada um com a sua aba. Criar,
 * ligar, desligar e apagar mudam o rascunho, como o resto da tela: valem no
 * Salvar. */
export function ModsPanel() {
  const t = useT();
  const ids = useAgentIds();
  const agents = useSettings((state) => state.agents);
  const saved = useSettings((state) => state.saved);
  const rights = useEntitlements();
  const canCreate = useFeature("customMods");
  const customs = ids.filter(isCustomMod).length;
  const savedIds = new Set<string>((() => { try { return (JSON.parse(saved) as { agents: AgentSettings[] }).agents.map((agent) => agent.id); } catch { return []; } })());

  const row = (agent: AgentSettings) => {
    const custom = isCustomMod(agent.id);
    const name = agentLabel(agent.id, agents);
    const feature = settingsTabFeature(agent.id);
    const locked = feature !== undefined && !allows(rights, feature);
    return (
      <li key={agent.id} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2.5">
        <Switch checked={agent.enabled} disabled={locked} aria-label={t("mods.enabled", { name })} onCheckedChange={(enabled) => updateAgent(agent.id, { enabled })} />
        <AgentIcon agent={agent.id} className="size-6 shrink-0" />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
            <span className="truncate">{name}</span>
            <Badge variant="outline">{t(isApiAgent(agent) ? "mods.kind.api" : "mods.kind.cli")}</Badge>
            <Badge variant={custom ? "secondary" : "outline"}>{t(custom ? "mods.origin.custom" : "mods.origin.builtIn")}</Badge>
            {custom && !savedIds.has(agent.id) && <Badge variant="warning">{t("settings.notSaved")}</Badge>}
            {locked && <Badge variant="outline">{t("mods.locked")}</Badge>}
          </span>
          <span className="truncate font-mono text-caption text-muted-foreground">{isApiAgent(agent) ? (agent.options as { baseUrl?: string }).baseUrl : agent.command}</span>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={() => setSettingsTab(agent.id)}><SettingsIcon aria-hidden="true" />{t("mods.configure")}</Button>
        {custom && (
          <ConfirmAction title={t("mods.remove.title", { name })} description={t("mods.remove.description")} confirm={t("mods.remove")} onConfirm={() => removeMod(agent.id)}>
            <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.remove.named", { name })} title={t("mods.remove.named", { name })}><Trash2Icon aria-hidden="true" /></Button>
          </ConfirmAction>
        )}
      </li>
    );
  };

  const shown = ids.map((id) => agents.find((agent) => agent.id === id)).filter((agent): agent is AgentSettings => agent !== undefined);
  return (
    <div className="grid gap-5">
      <SettingsSection
        title={t("mods.title")}
        description={t("mods.description")}
        action={(
          <ModCreateDialog>
            <Button type="button" size="sm" data-tour="settings-mods-create" disabled={!canCreate || customs >= MAX_MODS} title={!canCreate ? t("plans.locked") : customs >= MAX_MODS ? t("mods.full", { max: MAX_MODS }) : undefined}>
              <PlusIcon aria-hidden="true" />{t("mods.create")}
            </Button>
          </ModCreateDialog>
        )}
      >
        <ul className="grid gap-2">
          {shown.filter((agent) => !isCustomMod(agent.id)).map(row)}
        </ul>
        <h4 className="pt-1 text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("mods.yours")}</h4>
        {customs === 0
          ? <p className="text-sm text-muted-foreground">{t(canCreate ? "mods.empty" : "mods.empty.locked")}</p>
          : <ul className="grid gap-2">{shown.filter((agent) => isCustomMod(agent.id)).map(row)}</ul>}
        <p className="text-xs leading-snug text-muted-foreground">{t("mods.notes")}</p>
      </SettingsSection>
    </div>
  );
}
