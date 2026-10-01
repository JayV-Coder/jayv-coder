import { useShallow } from "zustand/react/shallow";
import { COMPLEXITIES, type AgentId } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { AGENTS, discardChanges, isDirty, problems, restoreCoreDefaults, saveSettings, useSettings } from "@/modules/settings";
import { AgentIcon, GridIcon, LoadingNote, LogoIcon } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { AGENT_NAMES, AgentPanel, AppPanel, JevPanel } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";

type Health = "off" | "ok" | "problem";

export function SettingsPage() {
  const t = useT();
  const { loaded, agents, models, saving, core, coreSnapshot } = useSettings(useShallow(({ loaded, agents, models, saving, core, coreSnapshot }) => ({ loaded, agents, models, saving, core, coreSnapshot })));
  const dirty = useSettings(isDirty);
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

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("settings.eyebrow")} title={t("settings.title")} description={t("settings.description")}>
        {dirty && <Badge variant="outline" className="border-amber-400/50 text-amber-400">{t("settings.unsaved")}</Badge>}
        <Button variant="ghost" disabled={saving} onClick={restoreCoreDefaults} title={t("settings.defaults.hint")}>{t("settings.defaults")}</Button>
        <Button variant="ghost" disabled={!dirty || saving} onClick={discardChanges}>{t("settings.discard")}</Button>
        <Button disabled={!dirty || saving || broken || noneEnabled} onClick={() => void saveSettings()}>{t("settings.save")}</Button>
      </PageHeading>

      {(broken || noneEnabled) && (
        <p role="alert" className="mb-5 rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-2.5 text-sm text-destructive">
          {t(noneEnabled ? "settings.noneEnabled" : "settings.fix")}
        </p>
      )}

      <Tabs defaultValue="jev" className="gap-5">
        <TabsList className="h-auto w-full justify-start gap-1 p-1">
          <TabsTrigger value="jev" className="flex-none gap-2.5 px-4 py-2">
            <LogoIcon className="size-5" />
            <span>{t("settings.tab.jev")}</span>
            {coreBroken && <span title={t("settings.health.problem")} aria-label={t("settings.health.problem")} className="size-2 rounded-full bg-destructive" />}
          </TabsTrigger>
          <TabsTrigger value="app" className="flex-none gap-2.5 px-4 py-2">
            <GridIcon className="size-5" />
            <span>{t("settings.tab.app")}</span>
          </TabsTrigger>
          <span aria-hidden="true" className="mx-1 h-5 w-px self-center bg-border" />
          {AGENTS.map((id) => {
            const state = health(id);
            return (
              <TabsTrigger key={id} value={id} className="flex-none gap-2.5 px-4 py-2">
                <AgentIcon agent={id} className="size-5" />
                <span>{AGENT_NAMES[id]}</span>
                <span
                  title={t(`settings.health.${state}`)}
                  aria-label={t(`settings.health.${state}`)}
                  className={cn("size-2 rounded-full", state === "ok" ? "bg-emerald-400" : state === "problem" ? "bg-destructive" : "bg-muted-foreground/40")}
                />
              </TabsTrigger>
            );
          })}
        </TabsList>
        <TabsContent value="jev"><JevPanel core={core} snapshot={coreSnapshot} /></TabsContent>
        <TabsContent value="app"><AppPanel core={core} snapshot={coreSnapshot} /></TabsContent>
        {AGENTS.map((id) => {
          const agent = agents.find((item) => item.id === id);
          return agent && (
            <TabsContent key={id} value={id}>
              <AgentPanel agent={agent} models={models.filter((model) => model.agent === id)} problems={found[id]} />
            </TabsContent>
          );
        })}
      </Tabs>
    </ScrollPage>
  );
}
