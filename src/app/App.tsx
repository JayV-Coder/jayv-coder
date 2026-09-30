import { useEffect } from "react";
import type { View } from "@/modules/core";
import { connectConversation } from "@/modules/conversation";
import { reportError } from "@/modules/feedback";
import { connectGate } from "@/modules/gate";
import { navigate, useNavigation } from "@/modules/navigation";
import { connectSettings } from "@/modules/settings";
import { connectSystem, loadStatus } from "@/modules/system";
import { checkForUpdate } from "@/modules/updates";
import { connectWorkspace, loadWorkspace } from "@/modules/workspace";
import { ChatPage, ChatsPage, GatePage, ProjectsPage, SettingsPage, StatusPage } from "@/components/pages";
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
  const view = useNavigation((state) => state.view);

  useEffect(() => {
    const disconnect = [connectWorkspace(), connectConversation(), connectGate(), connectSystem(), connectSettings()];
    navigate("projects");
    Promise.all([loadWorkspace(), loadStatus()]).catch(reportError);
    void checkForUpdate();
    return () => disconnect.forEach((off) => off());
  }, []);

  const Page = PAGES[view];
  return (
    <TooltipProvider>
      <AppShell><Page /></AppShell>
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
