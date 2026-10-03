import { clearChat } from "@/modules/conversation";
import { useT } from "@/modules/i18n";
import { useNavigation } from "@/modules/navigation";
import { chatTitle, findChat, findProject, useWorkspace } from "@/modules/workspace";
import { EraserIcon } from "lucide-react";
import { ConfirmAction } from "@/components/molecules";
import { Button } from "@/components/ui/button";

/** O cabeçalho das vistas de conversa, sistema, configuração e perfil: a
 * trilha de onde se está, escrita como um caminho, o título e, à direita, as ações da vista. */
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
    <header className="flex h-11 flex-none items-center justify-between gap-4 border-b border-border bg-background px-5">
      {/* A trilha se lê como um caminho: `projeto / chat`. */}
      <nav aria-label={t("header.breadcrumb")} className="flex min-w-0 items-center gap-2 text-body">
        <span className="shrink-0 text-muted-foreground">{eyebrow}</span>
        <span aria-hidden="true" className="shrink-0 text-faint">/</span>
        <h1 aria-current="page" className="truncate text-body font-medium">{title}</h1>
      </nav>
      {view === "chat" && chat && (
        // Limpar apaga o histórico inteiro e não se desfaz: pede confirmação.
        <ConfirmAction
          title={t("chat.clear.title")}
          description={t("chat.clear.description", { title: chatTitle(chat) })}
          confirm={t("header.clearChat")}
          onConfirm={() => void clearChat(chat.id)}
        >
          <Button variant="ghost" size="sm" className="text-muted-foreground">
            <EraserIcon aria-hidden="true" />
            {t("header.clearChat")}
          </Button>
        </ConfirmAction>
      )}
    </header>
  );
}
