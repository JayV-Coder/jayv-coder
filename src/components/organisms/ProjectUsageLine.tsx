import { useEffect } from "react";
import { useLocale, useT } from "@/modules/i18n";
import { formatCost, formatTokens, loadProjectUsage, useUsage } from "@/modules/usage";

/** Os últimos 30 dias do projeto, numa linha do cartão dele. */
export function ProjectUsageLine({ projectId }: { projectId: string }) {
  const t = useT();
  const locale = useLocale();
  const report = useUsage((state) => state.projects[projectId]);
  useEffect(() => { void loadProjectUsage(projectId); }, [projectId]);
  if (!report || report.totals.calls === 0) return null;
  const { totals } = report;
  const tokens = formatTokens(totals.inputTokens + totals.cacheReadTokens + totals.cacheWriteTokens + totals.outputTokens, locale);
  return (
    <span className="text-xs text-muted-foreground">
      {t("usage.project.month", { tokens })}{totals.costUsd !== null ? ` · ${formatCost(totals.costUsd, locale)}` : ""}
    </span>
  );
}
