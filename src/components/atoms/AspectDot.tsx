import type { ComponentProps } from "react";
import type { Aspect } from "@/modules/core";
import { cn } from "@/lib/utils";

/** A bolinha do semáforo. Sem aspecto, fica apagada. */
export function AspectDot({ aspect, className, ...props }: ComponentProps<"span"> & { aspect?: Aspect | null }) {
  return (
    <span
      data-aspect={aspect ?? undefined}
      className={cn("inline-block size-[7px] flex-none rounded-full bg-[var(--aspect,var(--faint))] shadow-[0_0_6px_var(--glow,transparent)]", className)}
      {...props}
    />
  );
}
