import { useT } from "@/modules/i18n";
import { setScope, sourceLabel, useUsage } from "@/modules/usage";
import { LoadingNote } from "@/components/atoms";
import { JevUsagePanel, QuotaPanel, UsageCharts, UsageSummary, UsageTable } from "@/components/organisms";
import { StatsToolbar } from "../components/StatsToolbar";
import { ScrollPage } from "@/components/templates";

/** As estatísticas de uso: o que o JayV gastou, o limite dos planos e o
 * desempenho do Jev, no escopo e no período escolhidos. */
export function StatsPage() {
  const t = useT();
  const report = useUsage((state) => state.report);
  const scope = useUsage((state) => state.scope);
  return (
    <ScrollPage>
      <StatsToolbar />
      {!report ? <LoadingNote>{t("usage.loading")}</LoadingNote> : (
        <>
          <UsageSummary totals={report.totals} />
          <QuotaPanel quotas={report.quotas} />
          {report.totals.calls === 0 && <p className="mb-6 text-sm text-muted-foreground">{t("usage.empty")}</p>}
          <UsageCharts daily={report.daily} byModel={report.byModel} />
          <JevUsagePanel report={report} />
          <UsageTable title={t("usage.table.bySource")} rows={report.bySource} name={(row) => sourceLabel(row.key, t)} />
          <UsageTable title={t("usage.table.byModel")} rows={report.byModel} name={(row) => `${sourceLabel(row.parent ?? "", t)} · ${row.label ?? row.key}`} />
          {scope.kind === "global" && (
            <UsageTable
              title={t("usage.table.byProject")}
              rows={report.byProject}
              name={(row) => row.label ?? t("usage.project.removed")}
              onPick={(row) => setScope({ kind: "project", id: row.key })}
            />
          )}
          {scope.kind !== "chat" && (
            <UsageTable
              title={t("usage.table.byChat")}
              rows={report.byChat}
              name={(row) => row.label || t("usage.chat.removed")}
              onPick={(row) => setScope({ kind: "chat", id: row.key })}
            />
          )}
        </>
      )}
    </ScrollPage>
  );
}
