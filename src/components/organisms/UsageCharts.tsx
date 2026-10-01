import { Bar, BarChart, CartesianGrid, Legend, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { UsageBreakdown, UsageDay } from "@/modules/core";
import { useLocale, useT } from "@/modules/i18n";
import { formatTokens, sourceColor, sourceLabel } from "@/modules/usage";
import { Card } from "@/components/ui/card";

const AXIS = { stroke: "#5c665e", fontSize: 11 };
const TOOLTIP = { contentStyle: { background: "#111512", border: "1px solid #2b322d", fontSize: 12 }, cursor: { fill: "#ffffff0a" } };

/** Os tokens ao longo do tempo, empilhados por quem gastou, e o peso de
 * cada modelo no período. */
export function UsageCharts({ daily, byModel }: { daily: UsageDay[]; byModel: UsageBreakdown[] }) {
  const t = useT();
  const locale = useLocale();
  const sources = [...new Set(daily.map((row) => row.source))];
  const days = [...new Set(daily.map((row) => row.day))].map((day) => {
    const point: Record<string, string | number> = { day };
    for (const row of daily.filter((found) => found.day === day)) point[row.source] = row.inputTokens + row.outputTokens;
    return point;
  });
  const dayLabel = (day: string) => new Intl.DateTimeFormat(locale, { day: "2-digit", month: "short" }).format(new Date(`${day}T12:00:00`));
  const models = byModel.slice(0, 10).map((row) => ({
    name: `${sourceLabel(row.parent ?? "", t)} · ${row.label ?? row.key}`,
    input: row.totals.inputTokens,
    output: row.totals.outputTokens,
  }));
  const tokens = (value: number) => formatTokens(value, locale);

  if (days.length === 0) return null;
  return (
    <section className="mb-6 grid gap-4 xl:grid-cols-2">
      <Card className="gap-3 px-5 py-4">
        <h3 className="text-sm font-bold">{t("usage.chart.daily")}</h3>
        <ResponsiveContainer width="100%" height={240}>
          <BarChart data={days}>
            <CartesianGrid stroke="#1e242a" vertical={false} />
            <XAxis dataKey="day" tickFormatter={dayLabel} {...AXIS} />
            <YAxis tickFormatter={tokens} width={56} {...AXIS} />
            <Tooltip {...TOOLTIP} labelFormatter={(day) => dayLabel(String(day))} formatter={(value, name) => [tokens(Number(value)), sourceLabel(String(name), t)]} />
            <Legend formatter={(name) => sourceLabel(String(name), t)} wrapperStyle={{ fontSize: 11 }} />
            {sources.map((source) => <Bar key={source} dataKey={source} stackId="tokens" fill={sourceColor(source)} />)}
          </BarChart>
        </ResponsiveContainer>
      </Card>
      <Card className="gap-3 px-5 py-4">
        <h3 className="text-sm font-bold">{t("usage.chart.models")}</h3>
        <ResponsiveContainer width="100%" height={240}>
          <BarChart data={models} layout="vertical">
            <CartesianGrid stroke="#1e242a" horizontal={false} />
            <XAxis type="number" tickFormatter={tokens} {...AXIS} />
            <YAxis type="category" dataKey="name" width={150} {...AXIS} />
            <Tooltip {...TOOLTIP} formatter={(value, name) => [tokens(Number(value)), name === "input" ? t("usage.input") : t("usage.output")]} />
            <Bar dataKey="input" stackId="model" fill="#6aa9ff" />
            <Bar dataKey="output" stackId="model" fill="#7bd985" />
          </BarChart>
        </ResponsiveContainer>
      </Card>
    </section>
  );
}
