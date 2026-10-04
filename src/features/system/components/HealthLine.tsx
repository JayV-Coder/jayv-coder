import type { ReactNode } from "react";
import type { Health } from "../report";
import { cn } from "@/lib/utils";

const DOT: Record<Health, string> = { ok: "bg-go", warn: "bg-warning", fail: "bg-destructive", off: "bg-muted-foreground/50" };
const TEXT: Record<Health, string> = { ok: "text-muted-foreground", warn: "text-warning", fail: "text-destructive", off: "text-muted-foreground" };

/** Uma linha de saúde da página Sistema: a bolinha da cor do estado, o nome
 * da peça, o que se sabe dela e, à direita, o que fazer. */
export function HealthLine({ health, label, detail, title, children }: {
  health: Health;
  label: string;
  detail: ReactNode;
  title?: string;
  children?: ReactNode;
}) {
  return (
    <li className="flex min-h-10 items-center gap-3 border-b border-border/60 py-2 last:border-b-0">
      <span aria-hidden="true" className={cn("size-2 shrink-0 rounded-full", DOT[health])} />
      <span className="w-36 shrink-0 truncate text-sm font-medium">{label}</span>
      <span title={title} className={cn("min-w-0 flex-1 truncate font-mono text-xs", TEXT[health])}>{detail}</span>
      {children}
    </li>
  );
}
