import { clearChat } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { useNavigation } from "@/modules/navigation";
import { chatTitle, findChat, findProject, useWorkspace } from "@/modules/workspace";
import { ChevronRightIcon } from "lucide-react";
import { Button } from "@/components/ui/button";

/** O cabeçalho das vistas de conversa, sistema, configuração e perfil: a
 * trilha de onde se está, o título e, à direita, as ações da vista. */
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
    <header className="flex h-14 flex-none items-center justify-between gap-4 border-b border-border bg-background px-6">
      <nav aria-label={t("header.breadcrumb")} className="flex min-w-0 items-center gap-1.5 text-sm">
        <span className="shrink-0 text-muted-foreground">{eyebrow}</span>
        <ChevronRightIcon aria-hidden="true" className="size-4 shrink-0 text-muted-foreground rtl:-scale-x-100" />
        <h1 aria-current="page" className="truncate text-sm font-semibold tracking-tight">{title}</h1>
      </nav>
      {view === "chat" && chat && (
        <Button variant="outline" size="sm" onClick={() => void clearChat(chat.id)}>{t("header.clearChat")}</Button>
      )}
    </header>
  );
}
