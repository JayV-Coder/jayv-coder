import type { UsageBreakdown, UsageReport } from "@/modules/core";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { formatDuration, formatTokens, sourceLabel } from "@/modules/usage";
import { Card } from "@/components/ui/card";

const WORK: [string, Key][] = [
  ["entry:pass", "usage.jev.work.entryPass"],
  ["entry:ask", "usage.jev.work.entryAsk"],
  ["entry:block", "usage.jev.work.entryBlock"],
  ["exit:cleared", "usage.jev.work.exitCleared"],
  ["exit:held", "usage.jev.work.exitHeld"],
  ["cache_hit", "usage.jev.work.cacheHit"],
  ["cache_miss", "usage.jev.work.cacheMiss"],
  ["secret_redacted", "usage.jev.work.secretRedacted"],
  ["file_withheld", "usage.jev.work.fileWithheld"],
  ["session_resumed", "usage.jev.work.sessionResumed"],
];
// O contexto cortado pelo orçamento não entra: era texto que o próprio Jev
// escolheu e depois tirou, não token que o modelo gastaria.
const SAVED: [string, Key][] = [["blocked", "usage.jev.saved.blocked"]];

/** O desempenho do Jev em três blocos: o que ele fez (contado), o que ele
 * custou (informado pela função) e o que ele poupou (sempre estimado, e por
 * isso à parte, nunca somado ao resto). */
export function JevUsagePanel({ report }: { report: UsageReport }) {
  const t = useT();
  const locale = useLocale();
  const stages: UsageBreakdown[] = report.bySource.filter((row) => row.key.startsWith("jev:"));
  const saved = SAVED.reduce((sum, [kind]) => sum + (report.jev.saved[kind] ?? 0), 0);
  const count = (value: number) => formatTokens(Math.round(value), locale);

  return (
    <section className="mb-6">
      <h3 className="mb-3 text-sm font-semibold">{t("usage.jev.title")}</h3>
      <div className="grid gap-4 xl:grid-cols-3">
        <Card className="gap-2 px-5 py-4">
          <strong className="text-xs text-muted-foreground">{t("usage.jev.work")}</strong>
          {WORK.map(([kind, label]) => (
            <Row key={kind} label={t(label)} value={count(report.jev.work[kind] ?? 0)} />
          ))}
        </Card>
        <Card className="gap-2 px-5 py-4">
          <strong className="text-xs text-muted-foreground">{t("usage.jev.cost")}</strong>
          {stages.length === 0 ? <p className="text-xs text-muted-foreground">{t("usage.empty")}</p> : stages.map((row) => (
            <Row
              key={row.key}
              label={sourceLabel(row.key, t)}
              value={t("usage.jev.cost.value", { calls: count(row.totals.calls), input: count(row.totals.inputTokens), output: count(row.totals.outputTokens), time: formatDuration(row.totals.durationMs, locale) })}
            />
          ))}
        </Card>
        <Card className="gap-2 px-5 py-4">
          <strong className="text-xs text-muted-foreground">{t("usage.jev.saved")}</strong>
          <p className="text-xs text-warning">{t("usage.jev.saved.hint")}</p>
          {SAVED.map(([kind, label]) => <Row key={kind} label={t(label)} value={`≈ ${count(report.jev.saved[kind] ?? 0)}`} />)}
          <Row label={t("usage.jev.saved.total")} value={`≈ ${count(saved)}`} strong />
        </Card>
      </div>
    </section>
  );
}

function Row({ label, value, strong }: { label: string; value: string; strong?: boolean }) {
  return (
    <span className="flex items-baseline justify-between gap-3 border-t border-border pt-1.5 text-xs">
      <span className="text-foreground">{label}</span>
      <span className={strong ? "font-semibold tabular-nums" : "tabular-nums text-muted-foreground"}>{value}</span>
    </span>
  );
}
