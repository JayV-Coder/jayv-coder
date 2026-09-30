import { useT, type Key } from "@/modules/i18n";
import type { Layout } from "@/modules/workspace";
import { GridIcon, ListIcon } from "@/components/atoms";
import { cn } from "@/lib/utils";

const OPTIONS: [Layout, Key, typeof GridIcon][] = [["grid", "layout.grid", GridIcon], ["list", "layout.list", ListIcon]];

/** Grade ou lista: como os cartões de projeto se arrumam. */
export function LayoutSwitch({ value, onChange }: { value: Layout; onChange: (layout: Layout) => void }) {
  const t = useT();
  return (
    <div role="group" aria-label={t("layout.label")} className="flex rounded-[9px] border border-input bg-secondary p-0.5">
      {OPTIONS.map(([layout, label, Icon]) => (
        <button
          key={layout}
          type="button"
          aria-pressed={value === layout}
          title={t(label)}
          onClick={() => onChange(layout)}
          className={cn("flex items-center gap-1.5 rounded-[7px] px-2.5 py-1.5 text-xs text-muted-foreground", value === layout && "bg-accent text-accent-foreground")}
        >
          <Icon className="size-3.5" />
          {t(label)}
        </button>
      ))}
    </div>
  );
}
