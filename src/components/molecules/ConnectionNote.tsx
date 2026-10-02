import { useConnection } from "@/modules/connection";
import { useT } from "@/modules/i18n";

/** O que a sincronização ainda deve: só aparece quando há o que dizer. As
 * recusas de um chat ou projeto aparecem nele; aqui fica só o resto. */
export function ConnectionNote() {
  const t = useT();
  const { link, pending, refusals } = useConnection();
  const lines = [
    link === "offline" && t("connection.offline"),
    link === "expired" && t("connection.expired"),
    pending > 0 && link !== "online" && t("connection.pending", { count: pending }),
    refusals.unplaced > 0 && t("connection.failed", { count: refusals.unplaced }),
  ].filter(Boolean) as string[];
  if (lines.length === 0) return null;
  return (
    <div role="status" className="grid gap-0.5 px-1 text-caption text-warning">
      {lines.map((line) => <span key={line}>{line}</span>)}
    </div>
  );
}
