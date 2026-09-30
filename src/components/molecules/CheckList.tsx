import { Checkbox } from "@/components/ui/checkbox";
import { cn } from "@/lib/utils";

/** Um grupo de marcações sobre uma lista fixa. */
export function CheckList({ id, items, selected, onChange, tone = "default" }: {
  id: string; items: { value: string; label: string }[]; selected: string[]; onChange: (selected: string[]) => void; tone?: "default" | "danger";
}) {
  const toggle = (value: string, on: boolean) => onChange(on ? [...selected, value] : selected.filter((item) => item !== value));
  return (
    <div className="flex flex-wrap gap-2">
      {items.map((item) => {
        const on = selected.includes(item.value);
        return (
          <label
            key={item.value}
            htmlFor={`${id}-${item.value}`}
            className={cn(
              "flex cursor-pointer items-center gap-2 rounded-md border px-2.5 py-1.5 text-xs transition-colors",
              on ? (tone === "danger" ? "border-destructive/60 bg-destructive/10" : "border-primary/60 bg-primary/10") : "border-border/70 hover:border-border",
            )}
          >
            <Checkbox id={`${id}-${item.value}`} checked={on} onCheckedChange={(checked) => toggle(item.value, checked === true)} />
            {item.label}
          </label>
        );
      })}
    </div>
  );
}
