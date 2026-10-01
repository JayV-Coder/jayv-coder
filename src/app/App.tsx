import { useEffect } from "react";
import type { View } from "@/modules/core";
import { connectAuth, useAuth } from "@/modules/auth";
import { connectConnection } from "@/modules/connection";
import { connectI18n, useT } from "@/modules/i18n";
import { connectConversation } from "@/modules/conversation";
import { reportError } from "@/modules/feedback";
import { connectGate } from "@/modules/gate";
import { navigate, useNavigation } from "@/modules/navigation";
import { connectSettings } from "@/modules/settings";
import { connectSystem, loadStatus } from "@/modules/system";
import { checkForUpdate } from "@/modules/updates";
import { connectWorkspace, loadWorkspace } from "@/modules/workspace";
import { ChatPage, ChatsPage, GatePage, LoginPage, ProjectsPage, SettingsPage, StatusPage } from "@/components/pages";
import { AppShell } from "@/components/templates";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";

const PAGES: Record<View, () => React.JSX.Element> = {
  projects: ProjectsPage,
  chats: ChatsPage,
  chat: ChatPage,
  gate: GatePage,
  status: StatusPage,
  settings: SettingsPage,
};

/** Liga os módulos uma vez: cada um passa a ouvir o núcleo e o barramento por
 * conta própria. A tela só escolhe qual página mostrar. */
export function App() {
  const t = useT();
  const view = useNavigation((state) => state.view);
  const status = useAuth((state) => state.status);
  const userEmail = useAuth((state) => state.email);

  useEffect(() => {
    const disconnect = [connectI18n(), connectAuth(), connectConnection(), connectWorkspace(), connectConversation(), connectGate(), connectSystem(), connectSettings()];
    void checkForUpdate();
    return () => disconnect.forEach((off) => off());
  }, []);

  // Os dados são do usuário: cada login — ou troca de conta — recarrega tudo
  // do banco que o núcleo acabou de abrir.
  useEffect(() => {
    if (status !== "signedIn") return;
    navigate("projects");
    Promise.all([loadWorkspace(), loadStatus()]).catch(reportError);
  }, [status, userEmail]);

  const Page = PAGES[view];
  return (
    <TooltipProvider>
      {status === "signedIn"
        ? <AppShell><Page /></AppShell>
        : status === "loading"
          ? <p className="grid min-h-screen place-items-center text-sm text-muted-foreground">{t("auth.loading")}</p>
          : <LoginPage />}
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
