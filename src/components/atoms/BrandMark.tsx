import { cn } from "@/lib/utils";

/** A marca do aplicativo: na lateral e ao lado de cada resposta da conversa. */
export function BrandMark({ className }: { className?: string }) {
  return (
    <span aria-hidden="true" className={cn("grid size-8 shrink-0 place-items-center rounded-md border border-foreground/10 bg-accent font-mono text-base font-semibold text-accent-foreground", className)}>
      J
    </span>
  );
}
