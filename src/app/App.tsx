import { useEffect } from "react";
import type { View } from "@/modules/core";
import { connectAuth, useAuth } from "@/modules/auth";
import { announceChanges, useChangelog } from "@/modules/changelog";
import { connectConnection } from "@/modules/connection";
import { connectI18n, useT } from "@/modules/i18n";
import { connectConversation } from "@/modules/conversation";
import { reportError } from "@/modules/feedback";
import { connectGate } from "@/modules/gate";
import { navigate, useNavigation } from "@/modules/navigation";
import { clearProfile, loadProfile, useProfile } from "@/modules/profile";
import { clearNotifications, connectNotifications, loadNotifications } from "@/modules/notifications";
import { clearOrganizations, connectOrganizationDashboard, loadOrganizations } from "@/modules/organizations";
import { connectSettings } from "@/modules/settings";
import { connectUpdates, useUpdate } from "@/modules/updates";
import { connectTutorial } from "@/modules/tutorial";
import { connectLive } from "@/modules/live";
import { connectTray } from "@/modules/tray";
import { connectUsage, refreshQuotas } from "@/modules/usage";
import { connectEnvironments, finishSwitch, loadEnvironment } from "@/modules/environments";
import { connectWorkspace, loadWorkspace } from "@/modules/workspace";
import { connectFeatures, featurePages, loadStatus } from "@/features";
import { allows, clearEntitlements, startEntitlements, useEntitlements, VIEW_FEATURE } from "@/modules/plans";
import { ChatPage, ChatsPage, GatePage, LoginPage, NewPasswordPage, OrganizationPage, OrganizationsPage, ProfilePage, ProfileSetupPage, ProjectsPage, SecondFactorPage, SettingsPage } from "@/components/pages";
import { EnvironmentLoading, FeatureLocked, LaunchUpdate, McpDraftDialog, TutorialOverlay, UpdateBanner, UpdateDialog, WhatsNewDialog } from "@/components/organisms";
import { LoadingNote } from "@/components/atoms";
import { AppShell } from "@/components/templates";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";

const PAGES: Record<View, () => React.JSX.Element> = {
  ...featurePages,
  projects: ProjectsPage,
  chats: ChatsPage,
  chat: ChatPage,
  gate: GatePage,
  settings: SettingsPage,
  profile: ProfilePage,
  organizations: OrganizationsPage,
  organization: OrganizationPage,
};

/** O tutorial da primeira visita espera: a conta ainda não abriu, a atualização
 * ou as novidades estão na frente. */
function tutorialBlocked() {
  const update = useUpdate.getState();
  return useAuth.getState().status !== "signedIn" || update.launching || update.open || useChangelog.getState().open;
}

/** Liga os módulos uma vez: cada um passa a ouvir o núcleo e o barramento por
 * conta própria. A tela só escolhe qual página mostrar. */
export function App() {
  const t = useT();
  const view = useNavigation((state) => state.view);
  const status = useAuth((state) => state.status);
  const userEmail = useAuth((state) => state.email);
  const recovering = useAuth((state) => state.recovering);
  const profile = useProfile((state) => state.profile);
  const profileLoading = useProfile((state) => state.loading);

  useEffect(() => {
    const disconnect = [connectI18n(), connectAuth(), connectConnection(), connectWorkspace(), connectEnvironments(), connectConversation(), connectGate(), connectFeatures(), connectSettings(), connectUsage(), connectNotifications(), connectUpdates(), connectOrganizationDashboard(), connectLive(), connectTray(), connectTutorial(tutorialBlocked)];
    return () => disconnect.forEach((off) => off());
  }, []);

  // Os dados são do usuário: cada login — ou troca de conta — recarrega tudo
  // do banco que o núcleo acabou de abrir.
  useEffect(() => {
    if (status !== "signedIn") {
      // Sem conta aberta não há dados a esperar: o carregamento da troca sai.
      if (status !== "loading") finishSwitch();
      clearProfile();
      clearOrganizations();
      clearNotifications();
      clearEntitlements();
      return;
    }
    navigate("projects");
    loadProfile().catch(reportError);
    // Sem a migração das organizações (ou sem rede), o resto do app segue.
    loadOrganizations().catch((error) => console.error("organizations", error));
    // Idem sem a migração das notificações: o sino fica só com as do aparelho.
    loadNotifications().catch((error) => console.error("notifications", error));
    // Os recursos do plano: sem a migração (ou sem rede), tudo segue liberado.
    startEntitlements().catch((error) => console.error("plans", error));
    void loadEnvironment();
    Promise.all([loadWorkspace(), loadStatus()]).catch(reportError).finally(finishSwitch);
    // O limite dos planos é lido ao abrir: é da conta, e muda fora do JayV.
    void refreshQuotas();
    // Depois de uma atualização, o que mudou desde a última versão vista.
    void announceChanges();
  }, [status, userEmail]);

  const Page = PAGES[view];
  // A tela de um recurso fora do plano (ou desligado pelo admin) vira o aviso
  // com o caminho para os planos, por qualquer atalho que se chegue a ela.
  const locked = VIEW_FEATURE[view];
  const blocked = useEntitlements((state) => (locked ? !allows(state, locked) : false));
  const loading = <LoadingNote>{t("auth.loading")}</LoadingNote>;
  // Sem perfil (a leitura falhou ou a linha não existe), o app abre mesmo
  // assim: o passo de perfil é convite, não porta.
  const signedIn = recovering
    ? <NewPasswordPage />
    : profileLoading
      ? loading
      : profile && !profile.completedAt
        ? <ProfileSetupPage profile={profile} />
        : <AppShell>{blocked && locked ? <FeatureLocked feature={locked} /> : <Page />}</AppShell>;
  return (
    <TooltipProvider>
      {/* O aviso de versão nova fica acima de qualquer tela, logada ou não. */}
      <div className="flex h-screen flex-col">
        <UpdateBanner />
        <div className="min-h-0 flex-1 overflow-y-auto">
          {status === "signedIn" ? signedIn : status === "secondFactor" ? <SecondFactorPage /> : status === "loading" ? loading : <LoginPage />}
        </div>
      </div>
      <TutorialOverlay />
      <LaunchUpdate />
      <EnvironmentLoading />
      <UpdateDialog />
      <WhatsNewDialog />
      <McpDraftDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
