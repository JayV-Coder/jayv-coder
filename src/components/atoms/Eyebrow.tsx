import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** A linha pequena em caixa-alta que diz onde se está antes do título. */
export function Eyebrow({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn("mb-1.5 text-[10px] font-semibold tracking-[0.18em] text-[#82ce89] uppercase", className)} {...props} />;
}
