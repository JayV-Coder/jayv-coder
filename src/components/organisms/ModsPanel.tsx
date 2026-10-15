import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { BookOpenIcon, DownloadIcon, ExternalLinkIcon, GlobeIcon, PlusIcon, SearchIcon, SettingsIcon, Trash2Icon } from "lucide-react";
import { isApiAgent, isCustomMod, type AgentSettings, type ModManifest } from "@/modules/core";
import { useEnvironment } from "@/modules/environments";
import { reportError } from "@/modules/feedback";
import { formatSince, useLocale, useT, type Key } from "@/modules/i18n";
import {
  dependents, findMod, installMod, isInstalled, isOn, isRequired, KNOWN_POINTS, missingRequirements, modAgentIds, modFeatures, modPagePath, modsSitePath, modText,
  ownedAgents, pointsOf, setModEnabled, statusOf, uninstallMod, unknownPoints, usable, useMods,
} from "@/modules/mods";
import { inPlan, locks, useEntitlements, useFeature, type FeatureKey } from "@/modules/plans";
import { agentLabel, MAX_MODS, removeMod, setSettingsTab, useAgentIds, useSettings, type SettingsTab } from "@/modules/settings";
import { openDashboard, openSite, SITE_URL } from "@/modules/site";
import { AgentIcon, ModIcon } from "@/components/atoms";
import { ConfirmAction, SegmentedControl, SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";

/** O nome de um ponto de extensão na tela; o que esta tela não conhece é
 * "outro". */
const pointLabel = (point: string): Key => ((KNOWN_POINTS as readonly string[]).includes(point) ? `mods.point.${point}` : "mods.point.other") as Key;

/** Configurações › Mods: o gerenciador de mods, como o de um jogo. Todo mod
 * mora no repositório `jayv-coder-mods` e desce pelo catálogo; cada um traz
 * alguma coisa a um ou mais pontos de extensão do app (motores de LLM,
 * recursos, agentes…). Instalar, ligar, desligar e desinstalar mudam o
 * rascunho, como o resto da tela, e valem no Salvar. Os mods que a pessoa cria
 * (agentes de linha de comando ou de API) se criam e editam no site. */
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
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState<"all" | string>("all");
  const owned = ownedAgents(mods.catalog, mods.saved);
  const customs = ids.filter((id) => isCustomMod(id) && !owned.has(id)).length;
  const full = ids.filter(isCustomMod).length >= MAX_MODS;
  const savedIds = new Set<string>((() => { try { return (JSON.parse(saved) as { agents: AgentSettings[] }).agents.map((agent) => agent.id); } catch { return []; } })());
  const site = (create = false) => void openDashboard(modsSitePath(environment, create)).catch(reportError);
  const title = (id: string) => { const manifest = findMod(mods.catalog, id); return manifest ? modText(manifest.title, locale) : id; };
  const names = (list: string[]) => list.map(title).join(", ");

  const needle = query.trim().toLocaleLowerCase();
  const matches = (manifest: ModManifest) => (kind === "all" || pointsOf(manifest).includes(kind))
    && (!needle || [modText(manifest.title, locale), modText(manifest.summary, locale), manifest.id, manifest.author ?? ""].some((text) => text.toLocaleLowerCase().includes(needle)));

  /** A aba que o mod configura: a do primeiro agente dele que existe. */
  const tabOf = (manifest: ModManifest): SettingsTab | undefined => modAgentIds(manifest).find((id) => (ids as readonly string[]).includes(id)) as SettingsTab | undefined;

  /** A página do mod no site: o que ele faz, como habilitar e como usar. */
  const pageButton = (manifest: ModManifest, name: string) => SITE_URL && (
    <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.page.named", { name })} title={t("mods.page.named", { name })} onClick={() => void openSite(modPagePath(locale, manifest.id)).catch(reportError)}>
      <BookOpenIcon aria-hidden="true" />
    </Button>
  );

  const pointBadges = (manifest: ModManifest) => pointsOf(manifest).map((point) => <Badge key={point} variant="secondary">{t(pointLabel(point))}</Badge>);

  /** O que o plano faz com o mod: fora dele, ou travado ligado (todo recurso
   * que ele traz é travado e ele não traz mais nada). */
  const planOf = (manifest: ModManifest) => {
    const features = modFeatures(manifest);
    const lockedOn = features.length > 0 && pointsOf(manifest).length === 1 && features.every((feature) => locks(rights, feature));
    const outside = !lockedOn && manifest.plan !== null && !inPlan(rights, manifest.plan as FeatureKey);
    return { lockedOn, outside };
  };

  const installedRow = (id: string) => {
    const manifest = findMod(mods.catalog, id);
    const name = title(id);
    const status = statusOf(mods, id);
    const { lockedOn, outside } = manifest ? planOf(manifest) : { lockedOn: false, outside: false };
    const on = isOn(mods, id);
    const changed = (mods.saved.mods[id]?.enabled ?? null) !== (mods.draft.mods[id]?.enabled ?? null);
    const fresh = !(id in mods.saved.mods);
    const tab = manifest ? tabOf(manifest) : undefined;
    const needing = dependents(mods, id).map((other) => modText(other.title, locale));
    const missing = manifest ? missingRequirements(mods, manifest) : [];
    const newer = manifest && !usable(manifest, mods.appVersion, mods.points);
    // O essencial não se desliga nem se desinstala; o plano ainda manda no que
    // ele traz.
    const required = isRequired(mods.catalog, id);
    return (
      <li key={id} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2.5">
        <Switch
          checked={lockedOn || required || on}
          disabled={lockedOn || required || status === "absent"}
          title={required ? t("mods.required.hint") : undefined}
          aria-label={t("mods.enabled", { name })}
          onCheckedChange={(next) => setModEnabled(id, next)}
        />
        <ModIcon icon={manifest?.icon} className="size-6 text-foreground" />
        <div className="grid min-w-0 flex-1 gap-1">
          <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
            <span className="truncate">{name}</span>
            {manifest?.version && <Badge variant="outline" className="font-mono text-caption">{t("mods.version", { version: manifest.version })}</Badge>}
            {manifest && pointBadges(manifest)}
            {required && <Badge variant="outline" title={t("mods.required.hint")}>{t("mods.required")}</Badge>}
            {lockedOn && <Badge variant="outline">{t("mods.feature.locked")}</Badge>}
            {outside && <Badge variant="outline">{t("mods.locked")}</Badge>}
            {status === "absent" && <Badge variant="warning">{t("mods.feature.absent")}</Badge>}
            {newer && <Badge variant="warning">{manifest.minApp && unknownPoints(manifest, mods.points).length === 0 ? t("mods.needsApp", { version: manifest.minApp }) : t("mods.needsNewerApp")}</Badge>}
            {on && missing.length > 0 && <Badge variant="warning">{t("mods.requires.missing", { names: names(missing) })}</Badge>}
            {fresh && <Badge variant="warning">{t("mods.pending.install")}</Badge>}
            {!fresh && changed && <Badge variant="warning">{t("settings.notSaved")}</Badge>}
          </span>
          {manifest && <span className="text-xs leading-snug text-muted-foreground">{modText(manifest.summary, locale)}</span>}
          {manifest && (manifest.author || (manifest.requires ?? []).length > 0) && (
            <span className="text-caption text-muted-foreground">
              {[manifest.author ? t("mods.by", { author: manifest.author }) : null, (manifest.requires ?? []).length > 0 ? t("mods.requires", { names: names(manifest.requires ?? []) }) : null].filter(Boolean).join(" · ")}
            </span>
          )}
        </div>
        {tab && <Button type="button" variant="ghost" size="sm" onClick={() => setSettingsTab(tab)}><SettingsIcon aria-hidden="true" />{t("mods.configure")}</Button>}
        {manifest && pageButton(manifest, name)}
        {manifest?.homepage && (
          <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.homepage", { name })} title={t("mods.homepage", { name })} onClick={() => void openUrl(manifest.homepage!).catch(reportError)}>
            <GlobeIcon aria-hidden="true" />
          </Button>
        )}
        {!lockedOn && !required && (
          <ConfirmAction
            title={t("mods.uninstall.title", { name })}
            description={[t("mods.uninstall.description"), needing.length > 0 ? t("mods.uninstall.dependents", { names: needing.join(", ") }) : ""].filter(Boolean).join(" ")}
            confirm={t("mods.uninstall")}
            onConfirm={() => uninstallMod(id)}
          >
            <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.uninstall.named", { name })} title={t("mods.uninstall.named", { name })}><Trash2Icon aria-hidden="true" /></Button>
          </ConfirmAction>
        )}
      </li>
    );
  };

  const availableRow = (manifest: ModManifest) => {
    const name = modText(manifest.title, locale);
    const { outside } = planOf(manifest);
    const newer = !usable(manifest, mods.appVersion, mods.points);
    const leaving = manifest.id in mods.saved.mods;
    const why = outside ? t("plans.locked") : newer ? t("mods.needsNewerApp") : undefined;
    return (
      <li key={manifest.id} className="flex flex-wrap items-center gap-3 rounded-md border border-dashed border-border px-3 py-2.5">
        <ModIcon icon={manifest.icon} className="size-6 text-foreground" />
        <div className="grid min-w-0 flex-1 gap-1">
          <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
            <span className="truncate">{name}</span>
            {manifest.version && <Badge variant="outline" className="font-mono text-caption">{t("mods.version", { version: manifest.version })}</Badge>}
            {pointBadges(manifest)}
            {outside && <Badge variant="outline">{t("mods.locked")}</Badge>}
            {newer && <Badge variant="warning">{manifest.minApp && unknownPoints(manifest, mods.points).length === 0 ? t("mods.needsApp", { version: manifest.minApp }) : t("mods.needsNewerApp")}</Badge>}
            {leaving && <Badge variant="warning">{t("mods.pending.uninstall")}</Badge>}
          </span>
          <span className="text-xs leading-snug text-muted-foreground">{modText(manifest.summary, locale)}</span>
          {(manifest.author || (manifest.requires ?? []).length > 0) && (
            <span className="text-caption text-muted-foreground">
              {[manifest.author ? t("mods.by", { author: manifest.author }) : null, (manifest.requires ?? []).length > 0 ? t("mods.requires", { names: names(manifest.requires ?? []) }) : null].filter(Boolean).join(" · ")}
            </span>
          )}
        </div>
        {pageButton(manifest, name)}
        <Button type="button" variant="outline" size="sm" disabled={why !== undefined} title={why} onClick={() => installMod(manifest.id)}>
          <DownloadIcon aria-hidden="true" />{t("mods.install")}
        </Button>
      </li>
    );
  };

  /** Um mod que a pessoa criou no site: um agente de linha de comando ou de
   * API, com a aba dele. */
  const ownRow = (agent: AgentSettings) => {
    const name = agentLabel(agent.id, agents);
    return (
      <li key={agent.id} className="flex flex-wrap items-center gap-3 rounded-md border border-border px-3 py-2.5">
        <AgentIcon agent={agent.id} className="size-6 shrink-0" />
        <div className="grid min-w-0 flex-1 gap-0.5">
          <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
            <span className="truncate">{name}</span>
            <Badge variant="outline">{t(isApiAgent(agent) ? "mods.kind.api" : "mods.kind.cli")}</Badge>
            {!savedIds.has(agent.id) && <Badge variant="warning">{t("settings.notSaved")}</Badge>}
          </span>
          <span className="truncate font-mono text-caption text-muted-foreground">{isApiAgent(agent) ? (agent.options as { baseUrl?: string }).baseUrl : agent.command}</span>
        </div>
        <Button type="button" variant="ghost" size="sm" onClick={() => setSettingsTab(agent.id)}><SettingsIcon aria-hidden="true" />{t("mods.configure")}</Button>
        {SITE_URL && savedIds.has(agent.id) && (
          <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.editOnSite.named", { name })} title={t("mods.editOnSite.named", { name })} onClick={() => site()}>
            <ExternalLinkIcon aria-hidden="true" />
          </Button>
        )}
        <ConfirmAction title={t("mods.remove.title", { name })} description={t("mods.remove.description")} confirm={t("mods.remove")} onConfirm={() => removeMod(agent.id)}>
          <Button type="button" variant="ghost" size="icon-sm" aria-label={t("mods.remove.named", { name })} title={t("mods.remove.named", { name })}><Trash2Icon aria-hidden="true" /></Button>
        </ConfirmAction>
      </li>
    );
  };

  // Os instalados: os do catálogo na ordem dele e, por último, os que saíram
  // do catálogo (só se desinstalam).
  const catalog = mods.catalog ?? [];
  const installedIds = [
    ...catalog.filter((manifest) => isInstalled(mods, manifest.id) && matches(manifest)).map((manifest) => manifest.id),
    ...(mods.catalog === null ? [] : Object.keys(mods.draft.mods).filter((id) => !findMod(mods.catalog, id) && (!needle || id.includes(needle)))),
  ];
  const available = catalog.filter((manifest) => !isInstalled(mods, manifest.id) && matches(manifest));
  const own = ids.filter((id) => isCustomMod(id) && !owned.has(id)).map((id) => agents.find((agent) => agent.id === id)).filter((agent): agent is AgentSettings => agent !== undefined);
  const searching = needle.length > 0 || kind !== "all";
  // O filtro por ponto: os que esta tela conhece e algum mod do catálogo usa.
  const kinds = KNOWN_POINTS.filter((point) => catalog.some((manifest) => pointsOf(manifest).includes(point)));

  return (
    <div className="grid gap-5">
      <SettingsSection title={t("mods.title")} description={t("mods.description")}>
        <div data-tour="settings-mods-search" className="flex flex-wrap items-center gap-2">
          <div className="relative min-w-48 flex-1">
            <SearchIcon aria-hidden="true" className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input value={query} className="pl-9" placeholder={t("mods.search")} aria-label={t("mods.search")} onChange={(event) => setQuery(event.target.value)} />
          </div>
          {kinds.length > 1 && (
            <SegmentedControl
              label={t("mods.filter")}
              value={kind}
              options={[{ value: "all", label: t("mods.filter.all") }, ...kinds.map((point) => ({ value: point, label: t(`mods.filter.${point}` as Key) }))]}
              onChange={setKind}
            />
          )}
        </div>
        <h4 className="pt-1 text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("mods.installedTitle")}</h4>
        {mods.catalog === null
          ? <p className="text-sm text-muted-foreground">{t("mods.catalog.none")}</p>
          : installedIds.length === 0
            ? <p className="text-sm text-muted-foreground">{searching ? t("mods.noMatch") : t("mods.installedEmpty")}</p>
            : <ul className="grid gap-2">{installedIds.map(installedRow)}</ul>}
        {mods.catalog !== null && (
          <>
            <h4 className="pt-1 text-xs font-semibold tracking-wider text-muted-foreground uppercase">{t("mods.available")}</h4>
            {available.length === 0
              ? <p className="text-sm text-muted-foreground">{searching ? t("mods.noMatch") : t("mods.availableEmpty")}</p>
              : <ul className="grid gap-2">{available.map(availableRow)}</ul>}
          </>
        )}
      </SettingsSection>

      <SettingsSection
        title={t("mods.yours")}
        description={t("mods.yours.description")}
        action={SITE_URL ? (
          <Button type="button" size="sm" data-tour="settings-mods-create" disabled={!canCreate || full} title={!canCreate ? t("plans.locked") : full ? t("mods.full", { max: MAX_MODS }) : t("mods.create.hint")} onClick={() => site(true)}>
            <PlusIcon aria-hidden="true" />{t("mods.create")}<ExternalLinkIcon aria-hidden="true" className="opacity-70" />
          </Button>
        ) : undefined}
      >
        {customs === 0
          ? <p className="text-sm text-muted-foreground">{t(canCreate ? "mods.empty.site" : "mods.empty.locked")}</p>
          : <ul className="grid gap-2">{own.map(ownRow)}</ul>}
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
