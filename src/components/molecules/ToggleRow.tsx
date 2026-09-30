import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

/** Uma opção de ligar e desligar, com o que ela faz escrito do lado. */
export function ToggleRow({ id, label, hint, checked, onChange, disabled, className }: {
  id: string; label: string; hint?: string; checked: boolean; onChange: (checked: boolean) => void; disabled?: boolean; className?: string;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-4 rounded-lg border border-border/60 px-3.5 py-3", disabled && "opacity-55", className)}>
      <label htmlFor={id} className="grid gap-0.5">
        <span className="text-sm font-medium">{label}</span>
        {hint && <span className="text-[11.5px] leading-snug text-muted-foreground">{hint}</span>}
      </label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} disabled={disabled} className="mt-0.5" />
    </div>
  );
}
