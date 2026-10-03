import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** Uma tecla, como o terminal mostra os atalhos: mono, miúda, com a borda de
 * baixo mais grossa para lembrar uma tecla de verdade. */
export function Kbd({ className, ...props }: ComponentProps<"kbd">) {
  return (
    <kbd
      className={cn(
        "inline-flex h-4 min-w-4 items-center justify-center rounded-xs border border-b-2 border-border bg-card px-1 font-mono text-[11px] leading-none font-normal text-muted-foreground",
        className,
      )}
      {...props}
    />
  );
}
