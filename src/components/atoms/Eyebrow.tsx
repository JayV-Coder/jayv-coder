import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** A linha pequena em caixa-alta que diz onde se está antes do título. */
export function Eyebrow({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn("mb-1 font-mono text-caption font-medium tracking-wider text-muted-foreground uppercase", className)} {...props} />;
}
