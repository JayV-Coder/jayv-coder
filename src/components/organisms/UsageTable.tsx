import type { UsageBreakdown } from "@/modules/core";
import { useLocale, useT } from "@/modules/i18n";
import { formatCost, formatTokens } from "@/modules/usage";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";

/** Uma quebra do gasto (por projeto, chat ou modelo). Com `onPick`, a linha
 * vira atalho para recortar as estatísticas por ela. */
export function UsageTable({ title, rows, name, onPick }: {
  title: string;
  rows: UsageBreakdown[];
  name: (row: UsageBreakdown) => string;
  onPick?: (row: UsageBreakdown) => void;
}) {
  const t = useT();
  const locale = useLocale();
  if (rows.length === 0) return null;
  return (
    <section className="mb-6">
      <h3 className="mb-2 text-sm font-bold">{title}</h3>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("usage.table.name")}</TableHead>
            <TableHead className="text-end">{t("usage.input")}</TableHead>
            <TableHead className="text-end">{t("usage.output")}</TableHead>
            <TableHead className="text-end">{t("usage.cache.read")}</TableHead>
            <TableHead className="text-end">{t("usage.cost")}</TableHead>
            <TableHead className="text-end">{t("usage.calls")}</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.map((row) => (
            <TableRow key={row.key} className={onPick ? "cursor-pointer" : undefined} onClick={onPick ? () => onPick(row) : undefined}>
              <TableCell className="max-w-[320px] truncate">
                {name(row)}
                {row.totals.estimatedTokens > 0 && <span title={t("usage.estimated.hint")} className="ms-1.5 text-[#c9a86a]">≈</span>}
              </TableCell>
              <TableCell className="text-end tabular-nums">{formatTokens(row.totals.inputTokens, locale)}</TableCell>
              <TableCell className="text-end tabular-nums">{formatTokens(row.totals.outputTokens, locale)}</TableCell>
              <TableCell className="text-end tabular-nums">{formatTokens(row.totals.cacheReadTokens, locale)}</TableCell>
              <TableCell className="text-end tabular-nums">{formatCost(row.totals.costUsd, locale)}</TableCell>
              <TableCell className="text-end tabular-nums">{formatTokens(row.totals.calls, locale)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </section>
  );
}
