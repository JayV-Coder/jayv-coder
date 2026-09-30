import type { ComponentProps } from "react";
import { cn } from "@/lib/utils";

/** Um caminho no disco. Corta pela esquerda: o fim da pasta é o que importa. */
export function PathText({ className, ...props }: ComponentProps<"code">) {
  return <code className={cn("path-rtl block min-w-0 font-mono", className)} {...props} />;
}
