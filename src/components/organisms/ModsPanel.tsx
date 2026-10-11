import { DownloadIcon, ExternalLinkIcon, PlusIcon, SettingsIcon, Trash2Icon } from "lucide-react";
import { isApiAgent, isCustomMod, type AgentSettings, type ModManifest } from "@/modules/core";
import { useEnvironment } from "@/modules/environments";
import { reportError } from "@/modules/feedback";
import { formatSince, useLocale, useT, type Key } from "@/modules/i18n";
import {
  FEATURE_MODS, findMod, modOn, modsSitePath, modText, presence, setModOn, templates, usable, useMods, type FeatureMod,
} from "@/modules/mods";
import { allows, inPlan, locks, settingsTabFeature, useEntitlements, useFeature } from "@/modules/plans";
import { agentLabel, installTemplate, MAX_MODS, removeMod, setSettingsTab, templateAgentId, updateAgent, useAgentIds, useSettings } from "@/modules/settings";
import { openDashboard, SITE_URL } from "@/modules/site";
import { AgentIcon } from "@/components/atoms";
import { ConfirmAction, SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";

/** Configurações › Mods. Todo mod mora no repositório `jayv-coder-mods` e
 * desce pelo catálogo: as integrações com LLM (as do app, as que a pessoa cria
 * no site e os mods prontos do catálogo) e os recursos opcionais do app, que
 * cada um liga e desliga aqui. Tudo muda o rascunho, como o resto da tela, e
 * vale no Salvar. Criar e editar a definição de um mod é no site. */
export function ModsPanel() {
  const t = useT();
  const locale = useLocale();
  const ids = useAgentIds();
  const agents = useSettings((state) => state.agents);
  const saved = useSettings((state) => state.saved);
  const rights = useEntitlements();
  const mods = useMods();
  const environment = useEnvironment((state) => state.active);
  const canCreate = useFeature("customMods");
  const customs = ids.filter(isCustomMod).length;
  const full = customs >= MAX_MODS;
  const savedIds = new Set<string>((() => { try { return (JSON.parse(saved) as { agents: AgentSettings[] }).agents.map((agent) => agent.id); } catch { return []; } })());
  const site = (create = false) => void openDashboard(modsSitePath(environment, create)).catch(reportError);
  const summaryOf = (manifest: ModManifest | undefined) => (manifest ? modText(manifest.summary, locale) : "");

  const agentRow = (agent: AgentSettings) => {
    const custom = isCustomMod(agent.id);
    const name = agentLabel(agent.id, agents);
    const feature = settingsTabFeature(agent.id);
    const locked = feature !== undefined && !allows(rights, feature);
    const manifest = custom ? undefined : findMod(mods.catalog, "llm", agent.id);
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
          {manifest
            ? <span className="text-xs leading-snug text-muted-foreground">{summaryOf(manifest)}</span>
            : <span className="truncate font-mono text-caption text-muted-foreground">{isApiAgent(agent) ? (agent.options as { baseUrl?: string }).baseUrl : agent.command}</span>}
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={() => setSettingsTab(agent.id)}><SettingsIcon aria-hidden="true" />{t("mods.configure")}</Button>
        {custom && SITE_URL && savedIds.has(agent.id) && (
          <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.editOnSite.named", { name })} title={t("mods.editOnSite.named", { name })} onClick={() => site()}>
            <ExternalLinkIcon aria-hidden="true" />
          </Button>
        )}
        {custom && (
          <ConfirmAction title={t("mods.remove.title", { name })} description={t("mods.remove.description")} confirm={t("mods.remove")} onConfirm={() => removeMod(agent.id)}>
            <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.remove.named", { name })} title={t("mods.remove.named", { name })}><Trash2Icon aria-hidden="true" /></Button>
          </ConfirmAction>
        )}
      </li>
    );
  };

  const templateRow = (manifest: ModManifest) => {
    const name = modText(manifest.title, locale);
    const installed = agents.some((agent) => agent.id === templateAgentId(manifest));
    const newer = !usable(manifest, mods.appVersion);
    const why = !canCreate ? t("plans.locked") : newer ? t("mods.needsApp", { version: manifest.minApp ?? "" }) : full ? t("mods.full", { max: MAX_MODS }) : undefined;
    return (
      <li key={manifest.id} className="flex flex-wrap items-center gap-3 rounded-md border border-dashed border-border px-3 py-2.5">
        <AgentIcon agent={templateAgentId(manifest)} className="size-6 shrink-0" />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
            <span className="truncate">{name}</span>
            {manifest.template && <Badge variant="outline">{t(manifest.template.kind === "api" ? "mods.kind.api" : "mods.kind.cli")}</Badge>}
            {newer && <Badge variant="outline">{t("mods.needsApp", { version: manifest.minApp ?? "" })}</Badge>}
          </span>
          <span className="text-xs leading-snug text-muted-foreground">{summaryOf(manifest)}</span>
        </div>
        {installed
          ? <Badge variant="secondary">{t("mods.installed")}</Badge>
          : (
            <Button type="button" variant="outline" size="sm" disabled={why !== undefined} title={why} onClick={() => installTemplate(manifest, locale)}>
              <DownloadIcon aria-hidden="true" />{t("mods.install")}
            </Button>
          )}
      </li>
    );
  };

  const featureRow = (id: FeatureMod) => {
    const manifest = findMod(mods.catalog, "feature", id);
    const name = manifest ? modText(manifest.title, locale) : t(`feature.${id}.title` as Key);
    const summary = manifest ? modText(manifest.summary, locale) : t(`feature.${id}.detail` as Key);
    const where = presence(mods, "feature", id);
    const lockedOn = locks(rights, id);
    const outside = !inPlan(rights, id);
    const on = modOn(mods, id);
    const changed = (mods.saved[id] ?? null) !== (mods.states[id] ?? null);
    const status: Key | null = lockedOn ? "mods.feature.locked" : outside ? "mods.locked" : where === "absent" ? "mods.feature.absent" : where === "newer" ? "mods.needsApp" : null;
    const fixed = status !== null;
    return (
      <li key={id} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2.5">
        <Switch
          checked={lockedOn || (on && !outside && where !== "absent" && where !== "newer")}
          disabled={fixed}
          aria-label={t("mods.enabled", { name })}
          onCheckedChange={(next) => setModOn(id, next)}
        />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
            <span className="truncate">{name}</span>
            {status && <Badge variant="outline">{t(status, { version: manifest?.minApp ?? "" })}</Badge>}
            {changed && !fixed && <Badge variant="warning">{t("settings.notSaved")}</Badge>}
          </span>
          <span className="text-xs leading-snug text-muted-foreground">{summary}</span>
        </div>
      </li>
    );
  };

  const shown = ids.map((id) => agents.find((agent) => agent.id === id)).filter((agent): agent is AgentSettings => agent !== undefined);
  const ready = templates(mods.catalog);
  return (
    <div className="grid gap-5">
      <SettingsSection
        title={t("mods.title")}
        description={t("mods.description")}
        action={SITE_URL ? (
          <Button type="button" size="sm" data-tour="settings-mods-create" disabled={!canCreate || full} title={!canCreate ? t("plans.locked") : full ? t("mods.full", { max: MAX_MODS }) : t("mods.create.hint")} onClick={() => site(true)}>
            <PlusIcon aria-hidden="true" />{t("mods.create")}<ExternalLinkIcon aria-hidden="true" className="opacity-70" />
          </Button>
        ) : undefined}
      >
        <ul className="grid gap-2">
          {shown.filter((agent) => !isCustomMod(agent.id)).map(agentRow)}
        </ul>
        <h4 className="pt-1 text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("mods.yours")}</h4>
        {customs === 0
          ? <p className="text-sm text-muted-foreground">{t(canCreate ? "mods.empty.site" : "mods.empty.locked")}</p>
          : <ul className="grid gap-2">{shown.filter((agent) => isCustomMod(agent.id)).map(agentRow)}</ul>}
        {ready.length > 0 && (
          <>
            <h4 className="pt-1 text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("mods.templates")}</h4>
            <ul className="grid gap-2">{ready.map(templateRow)}</ul>
          </>
        )}
      </SettingsSection>

      <SettingsSection title={t("mods.features.title")} description={t("mods.features.description")}>
        <ul data-tour="settings-mods-features" className="grid gap-2">{FEATURE_MODS.map(featureRow)}</ul>
      </SettingsSection>

      <div className="grid gap-1.5">
        <p className="text-xs leading-snug text-muted-foreground">
          {mods.fetchedAt ? t("mods.catalog.from", { when: formatSince(mods.fetchedAt, locale) }) : t("mods.catalog.none")}
        </p>
        <p className="text-xs leading-snug text-muted-foreground">{t("mods.notes.site")}</p>
        {SITE_URL && (
          <Button type="button" variant="link" size="sm" className="justify-self-start px-0" onClick={() => site()}>
            {t("mods.site")}<ExternalLinkIcon aria-hidden="true" />
          </Button>
        )}
      </div>
    </div>
  );
}
