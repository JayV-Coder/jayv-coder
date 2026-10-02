import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { Button } from "@/components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

export const PAGE_SIZES = [10, 20, 50] as const;

/** Rodapé de uma lista longa: quantos itens por página, onde se está e as
 * setas para andar. `page` começa em zero. */
export function Pager({ id, page, size, total, onPage, onSize }: {
  id: string; page: number; size: number; total: number; onPage: (page: number) => void; onSize: (size: number) => void;
}) {
  const t = useT();
  const pages = Math.max(1, Math.ceil(total / size));
  const from = total === 0 ? 0 : page * size + 1;
  const to = Math.min(total, (page + 1) * size);

  return (
    <div className="flex flex-wrap items-center justify-between gap-3 text-[12.5px] text-muted-foreground">
      <label htmlFor={id} className="flex items-center gap-2">
        <span>{t("pager.perPage")}</span>
        <Select value={String(size)} onValueChange={(value) => onSize(Number(value))}>
          <SelectTrigger id={id} size="sm" className="w-[76px]"><SelectValue /></SelectTrigger>
          <SelectContent>
            {PAGE_SIZES.map((value) => <SelectItem key={value} value={String(value)}>{value}</SelectItem>)}
          </SelectContent>
        </Select>
      </label>
      <div className="flex items-center gap-3">
        <span className="tabular-nums">{t("pager.range", { from, to, total })}</span>
        <span className="tabular-nums">{t("pager.page", { page: page + 1, pages })}</span>
        <div className="flex gap-1">
          <Button variant="outline" size="icon-sm" disabled={page === 0} aria-label={t("pager.previous")} title={t("pager.previous")} onClick={() => onPage(page - 1)}>
            <ChevronLeftIcon />
          </Button>
          <Button variant="outline" size="icon-sm" disabled={page >= pages - 1} aria-label={t("pager.next")} title={t("pager.next")} onClick={() => onPage(page + 1)}>
            <ChevronRightIcon />
          </Button>
        </div>
      </div>
    </div>
  );
}
