import { RefreshCwIcon } from "lucide-react";
import type { UsageScope } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { refreshQuotas, setPeriod, setScope, useUsage, type Period } from "@/modules/usage";
import { chatTitle, chatsOf, useWorkspace } from "@/modules/workspace";
import { OptionSelect, type Option } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

const ALL = "*";

/** De quem e de quando é a conta: o escopo (tudo, um projeto, um chat dele)
 * e o período. Dentro de um projeto aberto o projeto já está escolhido: só
 * sobram os chats dele. */
export function StatsToolbar() {
  const t = useT();
  const { data, activeProjectId } = useWorkspace();
  const { scope, period, range, refreshingQuotas } = useUsage();
  const chat = scope.kind === "chat" ? data.chats.find((found) => found.id === scope.id) ?? null : null;
  const projectId = activeProjectId ?? (scope.kind === "project" ? scope.id : chat?.projectId ?? (scope.kind === "chat" ? null : ALL));

  const projects: Option<string>[] = [
    { value: ALL, label: t("usage.scope.global") },
    ...data.projects.map((project) => ({ value: project.id, label: project.name })),
  ];
  // O chat de outro computador, já apagado aqui, ainda tem conta própria.
  if (scope.kind === "chat" && !chat) projects.push({ value: scope.id, label: t("usage.chat.removed"), disabled: true });
  const chats: Option<string>[] = projectId && projectId !== ALL
    ? [{ value: ALL, label: t("usage.scope.allChats") }, ...chatsOf(data, projectId).map((found) => ({ value: found.id, label: chatTitle(found) || found.code }))]
    : [];
  if (activeProjectId && scope.kind === "chat" && !chat) chats.push({ value: scope.id, label: t("usage.chat.removed"), disabled: true });
  const periods: Option<Period>[] = (["today", "7d", "30d", "all", "custom"] as Period[]).map((value) => ({ value, label: t(`usage.period.${value}`) }));

  const pickProject = (value: string) => setScope(value === ALL ? { kind: "global" } : { kind: "project", id: value });
  const pickChat = (value: string) => setScope(value === ALL ? { kind: "project", id: projectId as string } : { kind: "chat", id: value } as UsageScope);

  return (
    <div className="mb-6 flex flex-wrap items-end gap-3">
      {!activeProjectId && (
        <label className="grid min-w-[200px] gap-1 text-xs text-muted-foreground">
          {t("usage.scope.project")}
          <OptionSelect value={projectId ?? (scope.kind === "chat" ? scope.id : ALL)} options={projects} onChange={pickProject} />
        </label>
      )}
      {chats.length > 0 && (
        <label className="grid min-w-[200px] gap-1 text-xs text-muted-foreground">
          {t("usage.scope.chat")}
          <OptionSelect value={scope.kind === "chat" ? scope.id : ALL} options={chats} onChange={pickChat} />
        </label>
      )}
      <label className="grid min-w-[150px] gap-1 text-xs text-muted-foreground">
        {t("usage.period")}
        <OptionSelect value={period} options={periods} onChange={(value) => setPeriod(value)} />
      </label>
      {period === "custom" && (
        <>
          <label className="grid gap-1 text-xs text-muted-foreground">
            {t("usage.period.from")}
            <Input type="date" value={range.from} max={range.to} onChange={(event) => event.target.value && setPeriod("custom", { ...range, from: event.target.value })} />
          </label>
          <label className="grid gap-1 text-xs text-muted-foreground">
            {t("usage.period.to")}
            <Input type="date" value={range.to} min={range.from} onChange={(event) => event.target.value && setPeriod("custom", { ...range, to: event.target.value })} />
          </label>
        </>
      )}
      <Button variant="outline" className="ms-auto" disabled={refreshingQuotas} onClick={() => void refreshQuotas()}>
        <RefreshCwIcon aria-hidden="true" className={refreshingQuotas ? "animate-spin motion-reduce:animate-none" : undefined} />
        {refreshingQuotas ? t("usage.quota.refreshing") : t("usage.quota.refresh")}
      </Button>
    </div>
  );
}
