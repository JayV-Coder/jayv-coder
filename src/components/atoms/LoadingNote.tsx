import type { ReactNode } from "react";
import { LoaderCircleIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/** O aviso de espera, no meio da vista que ainda não chegou: o spinner no
 * centro e, logo abaixo, a mensagem. `fill` ocupa a vista toda (páginas);
 * sem ele, o aviso cabe num painel ou cartão. */
export function LoadingNote({ children, fill = true, className }: { children: ReactNode; fill?: boolean; className?: string }) {
  return (
    <div
      role="status"
      aria-live="polite"
      className={cn("flex flex-col items-center justify-center gap-3 text-sm text-muted-foreground", fill ? "min-h-full flex-1 p-10" : "p-6", className)}
    >
      <LoaderCircleIcon aria-hidden="true" className="size-6 animate-spin motion-reduce:animate-none" />
      <p className="text-center">{children}</p>
    </div>
  );
}
