import type { ReactNode } from "react";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

/** Um campo de formulário com o nome em cima e, embaixo, a explicação ou o
 * erro — o erro no lugar da explicação, para o campo não pular de tamanho. */
export function FormField({ label, htmlFor, wide, hint, error, className, children }: {
  label: string; htmlFor?: string; wide?: boolean; hint?: string; error?: string | null; className?: string; children: ReactNode;
}) {
  return (
    <div className={cn("grid content-start gap-1.5", wide && "col-span-full", className)}>
      <Label htmlFor={htmlFor} className="text-xs font-normal text-muted-foreground">{label}</Label>
      {children}
      {error
        ? <p role="alert" className="text-[11.5px] leading-snug text-destructive">{error}</p>
        : hint && <p className="text-[11.5px] leading-snug text-muted-foreground/80">{hint}</p>}
    </div>
  );
}
