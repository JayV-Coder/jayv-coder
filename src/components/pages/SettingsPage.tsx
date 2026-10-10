import type { ReactNode } from "react";
import { useShallow } from "zustand/react/shallow";
import { PlugIcon, SparklesIcon } from "lucide-react";
import { COMPLEXITIES, type AgentId } from "@/modules/core";
import { allows, SETTINGS_TAB_FEATURE, useEntitlements } from "@/modules/plans";
import { environmentLabel, useEnvironment } from "@/modules/environments";
import { useT } from "@/modules/i18n";
import { useOrganizations } from "@/modules/organizations";
import { AGENTS, discardChanges, isDirty, problems, restoreDefaults, saveSettings, setSettingsTab, useSettings, useSettingsDirty, useSettingsTab, type SettingsTab } from "@/modules/settings";
import { AgentIcon, GridIcon, LoadingNote, LogoIcon } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { AGENT_NAMES, AgentPanel, AppPanel, FeatureLocked, JevPanel, McpPanel, SkillsPanel } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";

type Health = "off" | "ok" | "problem";

export function SettingsPage() {
  const t = useT();
  const { loaded, agents, models, saving, core, coreSnapshot } = useSettings(useShallow(({ loaded, agents, models, saving, core, coreSnapshot }) => ({ loaded, agents, models, saving, core, coreSnapshot })));
  // Nada da tela vale antes do Salvar: o aviso e os botões olham todas as abas.
  const dirty = useSettingsDirty();
  const agentsDirty = useSettings(isDirty);
  const tab = useSettingsTab((state) => state.tab);
  const rights = useEntitlements();
  const organizations = useOrganizations((state) => state.list);
  const environment = useEnvironment((state) => state.active);
  // A aba cujo recurso o plano não tem abre o aviso do plano, não o painel.
  const panel = (id: SettingsTab, content: ReactNode) => {
    const feature = SETTINGS_TAB_FEATURE[id];
    return feature && !allows(rights, feature) ? <FeatureLocked feature={feature} /> : content;
  };
  if (!loaded || !core || !coreSnapshot) return <LoadingNote>{t("settings.loading")}</LoadingNote>;

  const found = Object.fromEntries(AGENTS.map((id) => [id, problems({ agents, models }, id)])) as Record<AgentId, ReturnType<typeof problems>>;
  const health = (id: AgentId): Health => {
    if (Object.keys(found[id]).length > 0) return "problem";
    return agents.find((agent) => agent.id === id)?.enabled ? "ok" : "off";
  };
  const [minBudget, maxBudget] = coreSnapshot.budgetRange;
  const coreBroken = COMPLEXITIES.some((level) => !(core.budgets[level] >= minBudget && core.budgets[level] <= maxBudget));
  const broken = coreBroken || AGENTS.some((id) => health(id) === "problem");
  const noneEnabled = !agents.some((agent) => agent.enabled);
  // Agente ou Jev com problema só seguram o Salvar quando são eles que mudaram:
  // o idioma, o tema, o MCP e as skills se salvam mesmo sem agente ligado.
  const blocked = agentsDirty && (broken || noneEnabled);

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("settings.eyebrow")} title={t("settings.title")} description={
        <span className="grid gap-2">
          <span>{t("settings.description")}</span>
          <span>{t("settings.saveHint")}</span>
          {/* Cada ambiente tem as suas configurações: o selo diz de qual são estas. */}
          {organizations.length > 0 && <span><Badge variant="outline">{t("environment.current", { name: environmentLabel(environment, organizations, t("environment.personal")) })}</Badge></span>}
        </span>
      }>
        {/* O aviso fica em cima dos botões, num lugar que existe mesmo sem
            alteração: aparecer não empurra os botões para baixo nem para o lado. */}
        <div className="grid justify-items-end gap-1.5">
          <Badge variant="warning" aria-hidden={!dirty} className={cn(!dirty && "invisible")}>{t("settings.unsaved")}</Badge>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button variant="ghost" disabled={saving} onClick={restoreDefaults} title={t("settings.defaults.hint")}>{t("settings.defaults")}</Button>
            <Button variant="ghost" disabled={!dirty || saving} onClick={discardChanges}>{t("settings.discard")}</Button>
            <Button disabled={!dirty || saving || blocked} onClick={() => void saveSettings()}>{t("settings.save")}</Button>
          </div>
        </div>
      </PageHeading>

      {(broken || noneEnabled) && (
        <p role="alert" className="mb-5 rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-2.5 text-sm text-destructive">
          {t(noneEnabled ? "settings.noneEnabled" : "settings.fix")}
        </p>
      )}

      <Tabs orientation="vertical" value={tab} onValueChange={(value) => setSettingsTab(value as SettingsTab)} className="gap-6">
        {/* As abas ficam numa coluna à esquerda, ao lado do conteúdo; a coluna
            acompanha a rolagem da página. */}
        <TabsList data-tour="settings-tabs" className="sticky top-0 h-auto w-52 shrink-0 gap-0.5 py-1 pr-1">
          <TabsTrigger value="app" className="flex-none gap-2.5 px-3 py-2">
            <GridIcon className="size-5" />
            <span>{t("settings.tab.app")}</span>
          </TabsTrigger>
          <TabsTrigger value="jev" className="flex-none gap-2.5 px-3 py-2">
            <LogoIcon className="size-5" />
            <span>{t("settings.tab.jev")}</span>
            {coreBroken && <span title={t("settings.health.problem")} aria-label={t("settings.health.problem")} className="ml-auto size-2 rounded-full bg-destructive" />}
          </TabsTrigger>
          <TabsTrigger value="mcp" className="flex-none gap-2.5 px-3 py-2">
            <PlugIcon aria-hidden="true" className="size-5" />
            <span>{t("settings.tab.mcp")}</span>
          </TabsTrigger>
          <TabsTrigger value="skills" className="flex-none gap-2.5 px-3 py-2">
            <SparklesIcon aria-hidden="true" className="size-5" />
            <span>{t("settings.tab.skills")}</span>
          </TabsTrigger>
          <span aria-hidden="true" className="my-1.5 h-px w-full self-center bg-border" />
          {AGENTS.map((id) => {
            const state = health(id);
            return (
              <TabsTrigger key={id} value={id} className="flex-none gap-2.5 px-3 py-2">
                <AgentIcon agent={id} className="size-5" />
                <span>{AGENT_NAMES[id]}</span>
                <span
                  title={t(`settings.health.${state}`)}
                  aria-label={t(`settings.health.${state}`)}
                  className={cn("ml-auto size-2 rounded-full", state === "ok" ? "bg-success" : state === "problem" ? "bg-destructive" : "bg-muted-foreground/40")}
                />
              </TabsTrigger>
            );
          })}
        </TabsList>
        <TabsContent value="app" className="min-w-0"><AppPanel core={core} snapshot={coreSnapshot} /></TabsContent>
        <TabsContent value="jev" className="min-w-0"><JevPanel core={core} snapshot={coreSnapshot} /></TabsContent>
        <TabsContent value="mcp" className="min-w-0">{panel("mcp", <McpPanel />)}</TabsContent>
        <TabsContent value="skills" className="min-w-0">{panel("skills", <SkillsPanel />)}</TabsContent>
        {AGENTS.map((id) => {
          const agent = agents.find((item) => item.id === id);
          return agent && (
            <TabsContent key={id} value={id} className="min-w-0">
              {panel(id, <AgentPanel agent={agent} models={models.filter((model) => model.agent === id)} problems={found[id]} />)}
            </TabsContent>
          );
        })}
      </Tabs>
    </ScrollPage>
  );
}
