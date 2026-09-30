import { useSystem } from "@/modules/system";
import { useT } from "@/modules/i18n";
import { LoadingNote } from "@/components/atoms";
import { Metric } from "@/components/molecules";
import { DatabaseCard } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";

export function StatusPage() {
  const t = useT();
  const status = useSystem((state) => state.status);
  if (!status) return <LoadingNote>{t("status.loading")}</LoadingNote>;
  const metrics: [string, string | number][] = [
    [t("status.providers"), status.providers], [t("status.models"), status.models], [t("status.indexed"), status.indexed_files],
    [t("status.cache"), t("status.cache.value", { count: status.cache_entries })],
    [t("status.history"), t("status.history.value", { count: status.session_messages })],
    [t("status.version"), status.version],
  ];
  return (
    <ScrollPage>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(210px,1fr))] gap-4">
        {metrics.map(([label, value]) => <Metric key={label} label={label}>{value}</Metric>)}
        <DatabaseCard status={status} />
      </div>
    </ScrollPage>
  );
}
