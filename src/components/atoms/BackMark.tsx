import { cn } from "@/lib/utils";

/** A marca de voltar, a mesma do site: o `❯` do prompt virado para a
 * esquerda, em verde. Em idiomas da direita para a esquerda ele volta a
 * apontar para a direita, que é onde fica o "antes". */
export function BackMark({ className }: { className?: string }) {
  return <span aria-hidden="true" className={cn("inline-block rotate-180 font-semibold text-go rtl:rotate-0", className)}>❯</span>;
}
