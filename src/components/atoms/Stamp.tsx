import { formatClock, useLocale } from "@/modules/i18n";

/** A hora em que a portaria registrou alguma coisa. */
export function Stamp({ at }: { at: string }) {
  const locale = useLocale();
  const date = new Date(at);
  return (
    <time dateTime={Number.isNaN(date.valueOf()) ? undefined : date.toISOString()} className="font-gate-mono text-xs text-faint tabular-nums">
      {formatClock(at, locale)}
    </time>
  );
}
