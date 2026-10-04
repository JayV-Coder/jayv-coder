import type { SystemStatus } from "@/modules/core";
import { useLocale, useT } from "@/modules/i18n";
import { tableSummary } from "../report";
import { Eyebrow } from "@/components/atoms";
import { Card } from "@/components/ui/card";

/** O banco local: nome, total de registros e cada tabela, da maior para a
 * menor, com uma barra do tamanho dela perto da maior. */
export function DatabaseCard({ status }: { status: SystemStatus }) {
  const t = useT();
  const locale = useLocale();
  const number = new Intl.NumberFormat(locale);
  const { sorted, total, largest } = tableSummary(status.tables);
  return (
    <Card className="gap-3 px-5 py-5">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <div className="min-w-0">
          <Eyebrow>{t("db.title")}</Eyebrow>
          <strong title={status.database_path} className="text-xl">{status.database_name}</strong>
        </div>
        <span className="font-mono text-xs text-muted-foreground tabular-nums">{t("db.total", { count: total, rows: number.format(total) })}</span>
      </div>
      {sorted.length === 0
        ? <p className="text-sm text-muted-foreground">{t("db.empty")}</p>
        : (
          <ul aria-label={t("db.table")} className="grid gap-1.5">
            {sorted.map((table) => (
              <li key={table.name} className="grid grid-cols-[minmax(0,11rem)_minmax(0,1fr)_5rem] items-center gap-3 text-xs">
                <span className="truncate font-mono">{table.name}</span>
                <span aria-hidden="true" className="h-1.5 overflow-hidden rounded-full bg-secondary">
                  <span className="block h-full rounded-full bg-chart-1" style={{ width: `${table.rows && largest ? Math.max(2, (table.rows / largest) * 100) : 0}%` }} />
                </span>
                <span className="text-end font-mono tabular-nums">{number.format(table.rows)}</span>
              </li>
            ))}
          </ul>
        )}
    </Card>
  );
}
