import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface SegmentedOption<T extends string> { value: T; label: string; icon?: ReactNode }

/** Um grupo de botões em que um só fica marcado: a superfície neutra por
 * fora, o escolhido em relevo por dentro. */
export function SegmentedControl<T extends string>({ label, value, options, onChange, className }: {
  label: string;
  value: T;
  options: SegmentedOption<T>[];
  onChange: (value: T) => void;
  className?: string;
}) {
  return (
    <div role="group" aria-label={label} className={cn("inline-flex w-fit rounded-md border border-border bg-secondary p-0.5", className)}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          aria-pressed={value === option.value}
          title={option.label}
          onClick={() => onChange(option.value)}
          className={cn(
            "flex h-7 items-center gap-1.5 rounded-sm border border-transparent px-2.5 text-xs font-medium text-muted-foreground transition-colors outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring",
            value === option.value && "border-border bg-card text-foreground",
          )}
        >
          {option.icon && <span aria-hidden="true" className="inline-flex">{option.icon}</span>}
          {option.label}
        </button>
      ))}
    </div>
  );
}
