import { useT, type Key } from "@/modules/i18n";
import type { Layout } from "@/modules/workspace";
import { GridIcon, ListIcon } from "@/components/atoms";
import { SegmentedControl } from "./SegmentedControl";

const OPTIONS: [Layout, Key, typeof GridIcon][] = [["grid", "layout.grid", GridIcon], ["list", "layout.list", ListIcon]];

/** Grade ou lista: como os cartões de projeto se arrumam. */
export function LayoutSwitch({ value, onChange }: { value: Layout; onChange: (layout: Layout) => void }) {
  const t = useT();
  return (
    <SegmentedControl
      label={t("layout.label")}
      value={value}
      onChange={onChange}
      options={OPTIONS.map(([layout, label, Icon]) => ({ value: layout, label: t(label), icon: <Icon className="size-3.5" /> }))}
    />
  );
}
