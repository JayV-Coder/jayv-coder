import { useEffect, useMemo } from "react";
import { TALLY } from "@/modules/gate";
import { useT } from "@/modules/i18n";
import {
  loadDashboardGate, loadDashboardReport, openDashboard, projectOrgId, setDashboardFilter, setDashboardPeriod, useDashboard, useOrganizations,
} from "@/modules/organizations";
import { sourceLabel, type Period } from "@/modules/usage";
import { chatTitle, chatsOf, useWorkspace } from "@/modules/workspace";
import { reportError } from "@/modules/feedback";
import { GateInIcon, GateOutIcon, LoadingNote } from "@/components/atoms";
import { OptionSelect, TallyItem, type Option } from "@/components/molecules";
import { Input } from "@/components/ui/input";
import { EntryItem } from "./EntryItem";
import { ExitItem } from "./ExitItem";
import { GateLane } from "./GateLane";
import { JevUsagePanel } from "./JevUsagePanel";
import { UsageCharts } from "./UsageCharts";
import { UsageSummary } from "./UsageSummary";
import { UsageTable } from "./UsageTable";

const ALL = "*";

/** Os projetos deste computador que entraram na organização. */
function useOrganizationProjects(orgId: string) {
  const links = useOrganizations((state) => state.projects);
  const all = useWorkspace((state) => state.data.projects);
  return useMemo(() => all.filter((project) => projectOrgId(project, links) === orgId), [all, links, orgId]);
}

/** Abre o recorte da organização e devolve os projetos dela. A lista de ids
 * só muda de identidade quando muda de conteúdo, para não reler à toa. */
function useDashboardFor(orgId: string) {
  const projects = useOrganizationProjects(orgId);
  const key = projects.map((project) => project.id).join(",");
  useEffect(() => {
    openDashboard(orgId, key ? key.split(",") : []);
  }, [orgId, key]);
  return projects;
}

/** O filtro das duas abas: projeto da organização, chat dele e, nas
 * estatísticas, o período. */
function DashboardToolbar({ orgId, withPeriod }: { orgId: string; withPeriod: boolean }) {
  const t = useT();
  const data = useWorkspace((state) => state.data);
  const projects = useOrganizationProjects(orgId);
  const { filter, period, range } = useDashboard();
  const projectOptions: Option<string>[] = [
    { value: ALL, label: t("org.dashboard.allProjects") },
    ...projects.map((project) => ({ value: project.id, label: project.name })),
  ];
  const chatOptions: Option<string>[] = filter.projectId
    ? [{ value: ALL, label: t("usage.scope.allChats") }, ...chatsOf(data, filter.projectId).map((chat) => ({ value: chat.id, label: chatTitle(chat) || chat.code }))]
    : [];
  // O chat de outro computador, já apagado aqui, ainda tem conta própria.
  if (filter.chatId && !chatOptions.some((option) => option.value === filter.chatId)) {
    chatOptions.push({ value: filter.chatId, label: t("usage.chat.removed"), disabled: true });
  }
  const periods: Option<Period>[] = (["today", "7d", "30d", "all", "custom"] as Period[]).map((value) => ({ value, label: t(`usage.period.${value}`) }));

  return (
    <div className="mb-6 flex flex-wrap items-end gap-3">
      <label className="grid min-w-[200px] gap-1 text-xs text-muted-foreground">
        {t("usage.scope.project")}
        <OptionSelect value={filter.projectId ?? ALL} options={projectOptions}
          onChange={(value) => setDashboardFilter({ projectId: value === ALL ? null : value, chatId: null })} />
      </label>
      {chatOptions.length > 0 && (
        <label className="grid min-w-[200px] gap-1 text-xs text-muted-foreground">
          {t("usage.scope.chat")}
          <OptionSelect value={filter.chatId ?? ALL} options={chatOptions}
            onChange={(value) => setDashboardFilter({ projectId: filter.projectId, chatId: value === ALL ? null : value })} />
        </label>
      )}
      {withPeriod && (
        <label className="grid min-w-[150px] gap-1 text-xs text-muted-foreground">
          {t("usage.period")}
          <OptionSelect value={period} options={periods} onChange={(value) => setDashboardPeriod(value)} />
        </label>
      )}
      {withPeriod && period === "custom" && (
        <>
          <label className="grid gap-1 text-xs text-muted-foreground">
            {t("usage.period.from")}
            <Input type="date" value={range.from} max={range.to} onChange={(event) => event.target.value && setDashboardPeriod("custom", { ...range, from: event.target.value })} />
          </label>
          <label className="grid gap-1 text-xs text-muted-foreground">
            {t("usage.period.to")}
            <Input type="date" value={range.to} min={range.from} onChange={(event) => event.target.value && setDashboardPeriod("custom", { ...range, to: event.target.value })} />
          </label>
        </>
      )}
    </div>
  );
}

/** As estatísticas de uso dos projetos da organização neste computador,
 * por projeto e por chat. */
export function OrganizationStats({ orgId }: { orgId: string }) {
  const t = useT();
  const projects = useDashboardFor(orgId);
  const { orgId: ready, filter, period, range, projectIds, report } = useDashboard();
  useEffect(() => {
    if (ready === orgId) loadDashboardReport().catch(reportError);
  }, [ready, orgId, filter, period, range, projectIds]);

  if (projects.length === 0) return <p className="text-sm text-muted-foreground">{t("projects.org.empty")}</p>;
  return (
    <div>
      <p className="mb-4 text-sm text-muted-foreground">{t("org.dashboard.description")}</p>
      <DashboardToolbar orgId={orgId} withPeriod />
      {!report ? <LoadingNote>{t("usage.loading")}</LoadingNote> : (
        <>
          <UsageSummary totals={report.totals} />
          {report.totals.calls === 0 && <p className="mb-6 text-sm text-muted-foreground">{t("usage.empty")}</p>}
          <UsageCharts daily={report.daily} byModel={report.byModel} />
          <JevUsagePanel report={report} />
          <UsageTable title={t("usage.table.bySource")} rows={report.bySource} name={(row) => sourceLabel(row.key, t)} />
          <UsageTable title={t("usage.table.byModel")} rows={report.byModel} name={(row) => `${sourceLabel(row.parent ?? "", t)} · ${row.label ?? row.key}`} />
          {!filter.projectId && (
            <UsageTable
              title={t("usage.table.byProject")}
              rows={report.byProject}
              name={(row) => row.label ?? t("usage.project.removed")}
              onPick={(row) => setDashboardFilter({ projectId: row.key, chatId: null })}
            />
          )}
          {!filter.chatId && (
            <UsageTable
              title={t("usage.table.byChat")}
              rows={report.byChat}
              name={(row) => row.label || t("usage.chat.removed")}
              onPick={(row) => setDashboardFilter({ projectId: row.parent ?? filter.projectId, chatId: row.key })}
            />
          )}
        </>
      )}
    </div>
  );
}

/** A portaria dos projetos da organização neste computador: o placar e os
 * dois portões, recortados por projeto e por chat. */
export function OrganizationGate({ orgId }: { orgId: string }) {
  const t = useT();
  const projects = useDashboardFor(orgId);
  const { orgId: ready, filter, projectIds, feed } = useDashboard();
  useEffect(() => {
    if (ready === orgId) loadDashboardGate().catch(reportError);
  }, [ready, orgId, filter, projectIds]);

  if (projects.length === 0) return <p className="text-sm text-muted-foreground">{t("projects.org.empty")}</p>;
  return (
    <div>
      <p className="mb-4 text-sm text-muted-foreground">{t("org.dashboard.description")}</p>
      <DashboardToolbar orgId={orgId} withPeriod={false} />
      <div className="overflow-hidden rounded-lg border border-rail-2 bg-void font-plate text-foreground">
        <div className="flex flex-wrap justify-end border-b border-rail-2 bg-panel px-4 py-4">
          {TALLY.map(([key, aspect, label]) => <TallyItem key={key} aspect={aspect} count={feed.tally[key] ?? 0} label={t(label)} />)}
        </div>
        <div className="grid h-[min(70vh,720px)] min-h-0 grid-cols-1 grid-rows-2 xl:grid-cols-2 xl:grid-rows-1">
          <GateLane
            icon={<GateInIcon className="size-full" />}
            title={t("gate.entry.title")}
            description={t("gate.entry.description")}
            count={feed.entries.length}
            empty={t("org.gate.entry.empty")}
          >
            {feed.entries.map((check) => <EntryItem key={check.id} check={check} />)}
          </GateLane>
          <GateLane
            icon={<GateOutIcon className="size-full" />}
            title={t("gate.exit.title")}
            description={t("gate.exit.description")}
            count={feed.exits.length}
            empty={t("org.gate.exit.empty")}
          >
            {feed.exits.map((check) => <ExitItem key={check.id} check={check} />)}
          </GateLane>
        </div>
      </div>
    </div>
  );
}
