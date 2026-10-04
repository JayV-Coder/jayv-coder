import { useEffect } from "react";
import type { View } from "@/modules/core";
import { connectAuth, useAuth } from "@/modules/auth";
import { announceChanges } from "@/modules/changelog";
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
import { connectSystem, loadStatus } from "@/modules/system";
import { connectUpdates } from "@/modules/updates";
import { connectLive } from "@/modules/live";
import { connectTray } from "@/modules/tray";
import { connectUsage, refreshQuotas } from "@/modules/usage";
import { connectWorkspace, loadWorkspace } from "@/modules/workspace";
import { allows, clearEntitlements, startEntitlements, useEntitlements, VIEW_FEATURE } from "@/modules/plans";
import { AdminPage, ChatPage, ChatsPage, GatePage, LoginPage, NewPasswordPage, OrganizationPage, OrganizationsPage, PlansPage, ProfilePage, ProfileSetupPage, ProjectsPage, SecondFactorPage, SettingsPage, StatsPage, StatusPage } from "@/components/pages";
import { FeatureLocked, UpdateBanner, UpdateDialog, WhatsNewDialog } from "@/components/organisms";
import { AppShell } from "@/components/templates";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";

const PAGES: Record<View, () => React.JSX.Element> = {
  projects: ProjectsPage,
  chats: ChatsPage,
  chat: ChatPage,
  gate: GatePage,
  status: StatusPage,
  stats: StatsPage,
  settings: SettingsPage,
  profile: ProfilePage,
  organizations: OrganizationsPage,
  organization: OrganizationPage,
  plans: PlansPage,
  admin: AdminPage,
};

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
    const disconnect = [connectI18n(), connectAuth(), connectConnection(), connectWorkspace(), connectConversation(), connectGate(), connectSystem(), connectSettings(), connectUsage(), connectNotifications(), connectUpdates(), connectOrganizationDashboard(), connectLive(), connectTray()];
    return () => disconnect.forEach((off) => off());
  }, []);

  // Os dados são do usuário: cada login — ou troca de conta — recarrega tudo
  // do banco que o núcleo acabou de abrir.
  useEffect(() => {
    if (status !== "signedIn") {
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
    Promise.all([loadWorkspace(), loadStatus()]).catch(reportError);
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
  const loading = <p className="grid min-h-full place-items-center text-sm text-muted-foreground">{t("auth.loading")}</p>;
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
      <UpdateDialog />
      <WhatsNewDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  );
}
