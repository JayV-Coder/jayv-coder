import type { UsageReport } from "@/modules/core";
import { useLocale, useT } from "@/modules/i18n";
import { formatPercent, formatTokens, openStats } from "@/modules/usage";
import { EmptyText } from "@/components/atoms";
import { SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";

/** O que a conta fez nos últimos 30 dias, em poucos números. O detalhe fica
 * nas estatísticas, a um clique. */
export function AccountActivity({ report, projects, chats }: { report: UsageReport | null; projects: number; chats: number }) {
  const t = useT();
  const locale = useLocale();
  const totals = report?.totals;
  const success = !totals || totals.calls === 0 ? null : (totals.calls - totals.failures) / totals.calls;
  const numbers: [string, string][] = totals ? [
    [t("profile.activity.projects"), formatTokens(projects, locale)],
    [t("profile.activity.chats"), formatTokens(chats, locale)],
    [t("usage.turns"), formatTokens(totals.turns, locale)],
    [t("profile.activity.tokens"), formatTokens(totals.inputTokens + totals.outputTokens, locale)],
    [t("usage.success"), success === null ? "—" : formatPercent(success, locale)],
  ] : [];

  return (
    <SettingsSection
      title={t("profile.activity.title")}
      description={t("profile.activity.description")}
      action={<Button variant="outline" size="sm" onClick={() => openStats({ kind: "global" })}>{t("profile.activity.open")}</Button>}
    >
      {!totals ? <EmptyText role="status">{t("usage.loading")}</EmptyText> : (
        <dl className="grid grid-cols-2 gap-3">
          {numbers.map(([label, value]) => (
            <div key={label} className="rounded-lg border border-border/60 px-3.5 py-3 last:col-span-2">
              <dt className="text-[11.5px] text-muted-foreground">{label}</dt>
              <dd className="font-gate-mono text-lg font-bold tabular-nums">{value}</dd>
            </div>
          ))}
        </dl>
      )}
    </SettingsSection>
  );
}
