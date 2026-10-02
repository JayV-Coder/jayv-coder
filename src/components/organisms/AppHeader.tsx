import { clearChat } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { useNavigation } from "@/modules/navigation";
import { chatTitle, findChat, findProject, useWorkspace } from "@/modules/workspace";
import { Eyebrow } from "@/components/atoms";
import { Button } from "@/components/ui/button";

/** O cabeçalho das vistas de conversa, sistema, configuração e perfil. */
export function AppHeader() {
  const t = useT();
  const view = useNavigation((state) => state.view);
  const { data, activeProjectId, activeChatId } = useWorkspace();
  const chat = findChat(data, activeChatId);
  const project = findProject(data, activeProjectId);
  if (view !== "chat" && view !== "status" && view !== "stats" && view !== "settings" && view !== "profile") return null;

  const eyebrow = view === "chat" ? t("header.project", { name: project?.name ?? "" }) : view === "status" || view === "stats" ? t("header.observability") : view === "profile" ? t("header.account") : t("header.preferences");
  const title = view === "chat" ? chat ? chatTitle(chat) : t("header.selectChat") : view === "status" ? t("nav.system") : view === "stats" ? t("nav.stats") : view === "profile" ? t("nav.profile") : t("nav.settings");

  return (
    <header className="flex h-[88px] flex-none items-center justify-between border-b border-[#252a26] bg-[#0c0f0d99] px-[38px] py-5 backdrop-blur-md">
      <div className="min-w-0">
        <Eyebrow>{eyebrow}</Eyebrow>
        <h1 className="truncate text-[21px] font-bold">{title}</h1>
      </div>
      {view === "chat" && chat && (
        <Button variant="outline" onClick={() => void clearChat(chat.id)}>{t("header.clearChat")}</Button>
      )}
    </header>
  );
}
