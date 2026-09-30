import { useShallow } from "zustand/react/shallow";
import type { AgentId } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { AGENTS, discardChanges, isDirty, problems, saveSettings, useSettings } from "@/modules/settings";
import { AgentIcon, LoadingNote } from "@/components/atoms";
import { PageHeading } from "@/components/molecules";
import { AGENT_NAMES, AgentPanel } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { cn } from "@/lib/utils";

type Health = "off" | "ok" | "problem";

export function SettingsPage() {
  const t = useT();
  const { loaded, agents, models, saving } = useSettings(useShallow(({ loaded, agents, models, saving }) => ({ loaded, agents, models, saving })));
  const dirty = useSettings(isDirty);
  if (!loaded) return <LoadingNote>{t("settings.loading")}</LoadingNote>;

  const found = Object.fromEntries(AGENTS.map((id) => [id, problems({ agents, models }, id)])) as Record<AgentId, ReturnType<typeof problems>>;
  const health = (id: AgentId): Health => {
    if (Object.keys(found[id]).length > 0) return "problem";
    return agents.find((agent) => agent.id === id)?.enabled ? "ok" : "off";
  };
  const broken = AGENTS.some((id) => health(id) === "problem");
  const noneEnabled = !agents.some((agent) => agent.enabled);

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("settings.eyebrow")} title={t("settings.title")} description={t("settings.description")}>
        {dirty && <Badge variant="outline" className="border-amber-400/50 text-amber-400">{t("settings.unsaved")}</Badge>}
        <Button variant="ghost" disabled={!dirty || saving} onClick={discardChanges}>{t("settings.discard")}</Button>
        <Button disabled={!dirty || saving || broken || noneEnabled} onClick={() => void saveSettings()}>{t("settings.save")}</Button>
      </PageHeading>

      {(broken || noneEnabled) && (
        <p role="alert" className="mb-5 rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-2.5 text-sm text-destructive">
          {t(noneEnabled ? "settings.noneEnabled" : "settings.fix")}
        </p>
      )}

      <Tabs defaultValue={AGENTS[0]} className="gap-5">
        <TabsList className="h-auto w-full justify-start gap-1 p-1">
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
