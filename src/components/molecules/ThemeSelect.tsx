import { MonitorIcon, MoonIcon, SunIcon } from "lucide-react";
import { useT, type Key } from "@/modules/i18n";
import { setThemePreference, useTheme, type ThemePreference } from "@/modules/theme";
import { SegmentedControl } from "./SegmentedControl";

const OPTIONS: [ThemePreference, Key, typeof SunIcon][] = [["system", "theme.system", MonitorIcon], ["light", "theme.light", SunIcon], ["dark", "theme.dark", MoonIcon]];

/** Claro, escuro ou o do sistema. */
export function ThemeSelect() {
  const t = useT();
  const preference = useTheme((state) => state.preference);
  return (
    <SegmentedControl
      label={t("theme.label")}
      value={preference}
      onChange={setThemePreference}
      options={OPTIONS.map(([value, label, Icon]) => ({ value, label: t(label), icon: <Icon className="size-3.5" /> }))}
    />
  );
}
