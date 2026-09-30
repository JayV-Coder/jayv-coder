import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Um item de menu da lateral: marca à esquerda, nome, e aceso quando é a
 * vista aberta. */
export function NavItem({ active, mark, className, children, ...props }: ComponentProps<"button"> & { active?: boolean; mark?: ReactNode }) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      className={cn(
        "flex w-full items-center rounded-[9px] px-3 py-2.5 text-start text-sm text-[#8f9991] transition-colors hover:bg-accent hover:text-accent-foreground",
        active && "bg-accent text-accent-foreground",
        className,
      )}
      {...props}
    >
      <span className="inline-flex w-[25px] flex-none text-[#657068]">{mark}</span>
      {children}
    </button>
  );
}
