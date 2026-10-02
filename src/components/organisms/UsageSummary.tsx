import type { UsageTotals } from "@/modules/core";
import { useLocale, useT } from "@/modules/i18n";
import { estimatedShare, formatCost, formatDuration, formatPercent, formatTokens } from "@/modules/usage";
import { Metric } from "@/components/molecules";

/** Os números grandes do período: tokens, custo informado, pedidos e
 * acertos. O selo avisa quando parte da conta é estimada. */
export function UsageSummary({ totals }: { totals: UsageTotals }) {
  const t = useT();
  const locale = useLocale();
  const share = estimatedShare(totals);
  const successRate = totals.calls === 0 ? null : (totals.calls - totals.failures) / totals.calls;
  return (
    <section className="mb-6">
      {share > 0 && (
        <p role="note" className="mb-3 text-xs text-warning" title={t("usage.estimated.hint")}>
          {t("usage.estimated.share", { share: formatPercent(share, locale) })}
        </p>
      )}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-4">
        <Metric label={t("usage.input")}>{formatTokens(totals.inputTokens, locale)}</Metric>
        <Metric label={t("usage.output")}>{formatTokens(totals.outputTokens, locale)}</Metric>
        <Metric label={t("usage.cache")} title={t("usage.cache.hint")}>
          {formatTokens(totals.cacheReadTokens, locale)} / {formatTokens(totals.cacheWriteTokens, locale)}
        </Metric>
        <Metric label={t("usage.cost")} title={t("usage.cost.hint")}>{formatCost(totals.costUsd, locale)}</Metric>
        {totals.requests !== null && <Metric label={t("usage.requests")}>{formatTokens(totals.requests, locale)}</Metric>}
        <Metric label={t("usage.turns")}>{formatTokens(totals.turns, locale)}</Metric>
        <Metric label={t("usage.calls")}>{formatTokens(totals.calls, locale)}</Metric>
        <Metric label={t("usage.success")}>{successRate === null ? "—" : formatPercent(successRate, locale)}</Metric>
        <Metric label={t("usage.duration")}>{formatDuration(totals.durationMs, locale)}</Metric>
      </div>
    </section>
  );
}
