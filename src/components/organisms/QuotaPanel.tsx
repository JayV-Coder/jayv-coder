import type { QuotaView } from "@/modules/core";
import { formatSince, say, useLocale, useT } from "@/modules/i18n";
import { agentLabel, formatReset, useUsage, windowLabel } from "@/modules/usage";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";

const ORDER = ["claude", "codex", "copilot", "jev"];

/** O limite do plano de cada agente: quanto da janela já foi, quando ela
 * renova e de quando é a leitura. É da conta inteira, não só do JayV. */
export function QuotaPanel({ quotas }: { quotas: QuotaView[] }) {
  const t = useT();
  const locale = useLocale();
  const problems = useUsage((state) => state.quotaProblems);
  const agents = [...new Set([...quotas.map((quota) => quota.agent), ...problems.map((problem) => problem.agent)])]
    .sort((left, right) => ORDER.indexOf(left) - ORDER.indexOf(right));

  return (
    <section className="mb-6">
      <h3 className="mb-1 text-sm font-bold">{t("usage.quota.title")}</h3>
      <p className="mb-3 text-xs text-muted-foreground">{t("usage.quota.description")}</p>
      {agents.length === 0 ? <p className="text-sm text-muted-foreground">{t("usage.quota.none")}</p> : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4">
          {agents.map((agent) => {
            const readings = quotas.filter((quota) => quota.agent === agent);
            const problem = problems.find((found) => found.agent === agent)?.problem ?? null;
            return (
              <Card key={agent} className="gap-3 px-5 py-4">
                <strong className="text-sm">{agentLabel(agent)}</strong>
                {readings.map((quota) => <QuotaBar key={quota.window} quota={quota} locale={locale} t={t} />)}
                {problem && <p className="text-xs text-[#c9a86a]">{say(problem)}</p>}
                {readings[0] && <small className="text-[11px] text-muted-foreground">{t("usage.quota.readAt", { date: formatSince(readings[0].capturedAt) })}</small>}
              </Card>
            );
          })}
        </div>
      )}
    </section>
  );
}

function QuotaBar({ quota, locale, t }: { quota: QuotaView; locale: string; t: ReturnType<typeof useT> }) {
  const percent = quota.usedPercent;
  const tone = percent === null ? "bg-[#59646f]" : percent >= 95 ? "bg-[#ff8a7a]" : percent >= 80 ? "bg-[#e0b25c]" : "bg-[#7bd985]";
  return (
    <div className="grid gap-1">
      <span className="flex items-baseline justify-between gap-2 text-xs">
        <span>{windowLabel(quota.window, t)}{quota.agent === "jev" && quota.plan ? ` · ${quota.plan}` : ""}</span>
        <b className="font-gate-mono tabular-nums">{percent === null ? "—" : `${Math.round(percent)}%`}</b>
      </span>
      <span className="h-2 overflow-hidden rounded-full bg-[#1e242a]">
        <span className={cn("block h-full", tone)} style={{ width: `${Math.min(100, percent ?? 0)}%` }} />
      </span>
      <span className="flex justify-between gap-2 text-[11px] text-muted-foreground">
        {quota.resetsAt ? <span>{t("usage.quota.resets", { when: formatReset(quota.resetsAt, locale) })}</span> : <span />}
        {quota.plan && quota.agent !== "jev" && <span>{quota.plan}</span>}
      </span>
    </div>
  );
}
