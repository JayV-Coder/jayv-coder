import { useEffect } from "react";
import { useLocale, useT } from "@/modules/i18n";
import { estimatedShare, formatCost, formatTokens, loadChatUsage, openStats, useUsage } from "@/modules/usage";

/** O rodapé da conversa: o que este chat gastou e o que o Jev fez nele. Um
 * clique abre as estatísticas recortadas por ele. */
export function ChatUsageBar({ chatId }: { chatId: string }) {
  const t = useT();
  const locale = useLocale();
  const report = useUsage((state) => state.chats[chatId]);
  useEffect(() => { void loadChatUsage(chatId); }, [chatId]);
  if (!report || report.totals.calls === 0) return null;
  const { totals, jev } = report;
  const blocked = jev.work["entry:block"] ?? 0;
  return (
    <button
      type="button"
      onClick={() => openStats({ kind: "chat", id: chatId })}
      title={t("usage.chat.open")}
      className="mx-[max(40px,calc((100%-880px)/2))] mb-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-start text-[11px] text-[#6e7870] hover:text-[#c9d1cb]"
    >
      <span>{t("usage.chat.tokens", { input: formatTokens(totals.inputTokens + totals.cacheReadTokens + totals.cacheWriteTokens, locale), output: formatTokens(totals.outputTokens, locale) })}</span>
      {totals.costUsd !== null && <span>{formatCost(totals.costUsd, locale)}</span>}
      {blocked > 0 && <span>{t("usage.chat.blocked", { count: blocked })}</span>}
      {estimatedShare(totals) > 0 && <span className="text-[#c9a86a]">≈</span>}
    </button>
  );
}
