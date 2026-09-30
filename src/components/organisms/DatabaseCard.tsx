import type { SystemStatus } from "@/modules/core";
import { Card } from "@/components/ui/card";
import { useT } from "@/modules/i18n";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";

/** O banco local: nome e quantos registros cada tabela tem. */
export function DatabaseCard({ status }: { status: SystemStatus }) {
  const t = useT();
  return (
    <Card className="col-span-full gap-3 px-5 py-5">
      <small className="text-xs text-muted-foreground">{t("db.title")}</small>
      <strong title={status.database_path} className="text-xl">{status.database_name}</strong>
      <Table>
        <TableHeader><TableRow><TableHead>{t("db.table")}</TableHead><TableHead className="text-end">{t("db.rows")}</TableHead></TableRow></TableHeader>
        <TableBody>
          {status.tables.length === 0
            ? <TableRow><TableCell colSpan={2} className="text-muted-foreground">{t("db.empty")}</TableCell></TableRow>
            : status.tables.map((table) => (
              <TableRow key={table.name}>
                <TableHead scope="row" className="font-mono text-xs">{table.name}</TableHead>
                <TableCell className="text-end tabular-nums">{table.rows}</TableCell>
              </TableRow>
            ))}
        </TableBody>
      </Table>
    </Card>
  );
}
