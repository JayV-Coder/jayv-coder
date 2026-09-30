import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** O que se diz quando uma lista ainda não tem nada. */
export function EmptyText({ className, ...props }: ComponentProps<"p">) {
  return <p className={cn("text-sm leading-relaxed text-muted-foreground", className)} {...props} />;
}
