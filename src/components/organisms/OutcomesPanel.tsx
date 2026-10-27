import { useEffect } from "react";
import type { UsageReport } from "@/modules/core";
import { useLocale, useT } from "@/modules/i18n";
import { estimateMoney, formatCost, formatPercent, formatTokens, loadPrices, taskNumbers, usePrices } from "@/modules/usage";
import { Metric } from "@/components/molecules";

/** O resultado do período: objetivos (um chat é um objetivo), turnos por
 * objetivo, tarefas que o validador comprovou e quanto custou cada uma. O
 * dinheiro soma o custo informado pelas ferramentas e, no resto, a tabela de
 * preços do site. */
export function OutcomesPanel({ report }: { report: UsageReport }) {
  const t = useT();
  const locale = useLocale();
  const { prices, loaded } = usePrices();
  useEffect(() => {
    if (!loaded) void loadPrices();
  }, [loaded]);
  const money = estimateMoney(report.byModel, prices);
  const numbers = taskNumbers(report.outcomes, money);
  const outcomes = report.outcomes;
  const decimal = (value: number) => new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value);

  return (
    <section className="mb-6" data-tour="usage-outcomes">
      <h3 className="mb-1 text-sm font-semibold">{t("usage.outcomes.title")}</h3>
      <p className="mb-3 text-xs text-muted-foreground">{t("usage.outcomes.hint")}</p>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-4">
        <Metric label={t("usage.outcomes.objectives")}>{formatTokens(outcomes.objectives, locale)}</Metric>
        <Metric label={t("usage.outcomes.turnsPerObjective")}>{numbers.turnsPerObjective === null ? "—" : decimal(numbers.turnsPerObjective)}</Metric>
        <Metric label={t("usage.outcomes.verified")} title={t("usage.outcomes.verified.hint", { failed: String(outcomes.failed), unproven: String(outcomes.unproven) })}>
          {formatTokens(outcomes.verified, locale)}
          {numbers.verifiedRate !== null && <span className="ml-2 text-xs font-normal text-muted-foreground">{formatPercent(numbers.verifiedRate, locale)}</span>}
        </Metric>
        <Metric label={t("usage.outcomes.tokensPerVerified")}>{numbers.tokensPerVerified === null ? "—" : formatTokens(Math.round(numbers.tokensPerVerified), locale)}</Metric>
        <Metric label={t("usage.outcomes.money")} title={t("usage.outcomes.money.hint")}>{money.totalUsd === 0 ? "—" : formatCost(money.totalUsd, locale)}</Metric>
        <Metric label={t("usage.outcomes.moneyPerVerified")}>{numbers.usdPerVerified === null ? "—" : formatCost(numbers.usdPerVerified, locale)}</Metric>
      </div>
      {money.estimatedUsd > 0 && (
        <p className="mt-2 text-xs text-warning">{t("usage.outcomes.money.estimated", { value: formatCost(money.estimatedUsd, locale) })}</p>
      )}
      {money.unpricedModels.length > 0 && (
        <p className="mt-1 text-xs text-muted-foreground">{t("usage.outcomes.money.unpriced", { models: money.unpricedModels.join(", ") })}</p>
      )}
      {outcomes.verified + outcomes.failed + outcomes.unproven === 0 && (
        <p className="mt-2 text-xs text-muted-foreground">{t("usage.outcomes.noValidation")}</p>
      )}
    </section>
  );
}
