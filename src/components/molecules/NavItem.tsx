import type { ComponentProps, ReactNode } from "react";
import { shortcutLabel, shortcutText, type Shortcut } from "@/modules/commands";
import { cn } from "@/lib/utils";

/** Um item de menu da lateral, como uma linha de árvore no terminal: ícone,
 * nome e, à direita, o atalho de teclado quando há um. Aceso quando é a vista
 * aberta (fundo destacado, texto cheio e o traço verde-limão na borda). */
export function NavItem({ active, mark, shortcut, className, children, ...props }: ComponentProps<"button"> & { active?: boolean; mark?: ReactNode; shortcut?: Shortcut }) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      aria-keyshortcuts={shortcut ? shortcutLabel(shortcut).join("+").replace("⌘", "Meta").replace("Ctrl", "Control") : undefined}
      className={cn(
        "relative flex h-8 w-full items-center gap-2.5 rounded-xs px-2.5 text-start text-[13px] text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-1 focus-visible:ring-ring",
        active && "bg-sidebar-accent text-sidebar-foreground shadow-[inset_2px_0_0_var(--accent)]",
        className,
      )}
      {...props}
    >
      <span aria-hidden="true" className="inline-flex size-4 flex-none items-center justify-center [&_svg:not([class*='size-'])]:size-4">{mark}</span>
      {children}
      {shortcut && <span aria-hidden="true" className="ms-auto shrink-0 text-caption text-faint">{shortcutText(shortcut)}</span>}
    </button>
  );
}
