import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Um item de menu da lateral: ícone à esquerda, nome, e aceso quando é a
 * vista aberta (fundo destacado, texto cheio e o traço de destaque na borda). */
export function NavItem({ active, mark, className, children, ...props }: ComponentProps<"button"> & { active?: boolean; mark?: ReactNode }) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      className={cn(
        "relative flex h-9 w-full items-center gap-2.5 rounded-md border border-transparent px-2.5 text-start text-[13px] font-medium text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring",
        active && "border-sidebar-border bg-sidebar-accent text-sidebar-foreground before:absolute before:inset-y-2 before:start-0 before:w-0.5 before:rounded-full before:bg-foreground",
        className,
      )}
      {...props}
    >
      <span aria-hidden="true" className="inline-flex size-[18px] flex-none items-center justify-center [&_svg:not([class*='size-'])]:size-[18px]">{mark}</span>
      {children}
    </button>
  );
}
