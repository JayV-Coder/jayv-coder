import { useConnection } from "@/modules/connection";
import { useT } from "@/modules/i18n";

/** O que a sincronização ainda deve: só aparece quando há o que dizer. */
export function ConnectionNote() {
  const t = useT();
  const { link, pending, failed } = useConnection();
  const lines = [
    link === "offline" && t("connection.offline"),
    link === "expired" && t("connection.expired"),
    pending > 0 && link !== "online" && t("connection.pending", { count: pending }),
    failed > 0 && t("connection.failed", { count: failed }),
  ].filter(Boolean) as string[];
  if (lines.length === 0) return null;
  return (
    <div role="status" className="grid gap-0.5 px-1 text-[11px] text-[#c9a86a]">
      {lines.map((line) => <span key={line}>{line}</span>)}
    </div>
  );
}
