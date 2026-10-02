import { useState } from "react";
import { LayoutDashboardIcon, LinkIcon, ShieldCheckIcon, UserRoundIcon } from "lucide-react";
import { useAuth } from "@/modules/auth";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { saveProfile, useProfile, type AccountProfile } from "@/modules/profile";
import { useSettings } from "@/modules/settings";
import { useUsage } from "@/modules/usage";
import { useWorkspace } from "@/modules/workspace";
import { LoadingNote } from "@/components/atoms";
import { AccountActivity, ExpertisePanel, LinkedAccounts, ProfileCard, ProfileForm, QuotaPanel, SecurityPanel } from "@/components/organisms";
import { SettingsSection } from "@/components/molecules";
import { ScrollPage } from "@/components/templates";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";

/** A conta de quem usa o app. O card fica acima das abas; a visão geral (o
 * nível com que o Jev o trata, o que fez no último mês e quanto resta dos
 * planos dos agentes) abre primeiro, e dados pessoais, senha e contas
 * vinculadas ficam cada um na sua aba. */
export function ProfilePage() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const profile = useAuth((state) => state.profile);
  const snapshot = useSettings((state) => state.coreSnapshot);
  const account = useUsage((state) => state.account);
  const { data } = useWorkspace();
  const personal = useProfile((state) => state.profile);
  const [saving, setSaving] = useState(false);

  const save = (draft: AccountProfile) => {
    setSaving(true);
    saveProfile(draft).then(() => notify(t("profile.saved"))).catch(reportError).finally(() => setSaving(false));
  };

  return (
    <ScrollPage>
      <ProfileCard email={email} account={personal} profile={profile} expertise={snapshot?.expertise ?? null} />
      <Tabs defaultValue="overview" className="gap-5">
        <TabsList className="h-auto w-full justify-start gap-1 overflow-x-auto p-1">
          <TabsTrigger value="overview" className="flex-none gap-2.5 px-4 py-2">
            <LayoutDashboardIcon className="size-4" />
            <span>{t("profile.tab.overview")}</span>
          </TabsTrigger>
          <TabsTrigger value="data" className="flex-none gap-2.5 px-4 py-2">
            <UserRoundIcon className="size-4" />
            <span>{t("profile.tab.data")}</span>
          </TabsTrigger>
          <TabsTrigger value="security" className="flex-none gap-2.5 px-4 py-2">
            <ShieldCheckIcon className="size-4" />
            <span>{t("profile.tab.security")}</span>
          </TabsTrigger>
          <TabsTrigger value="linked" className="flex-none gap-2.5 px-4 py-2">
            <LinkIcon className="size-4" />
            <span>{t("profile.tab.linked")}</span>
          </TabsTrigger>
        </TabsList>
        <TabsContent value="overview">
        <div className="mb-6 grid items-start gap-5 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,1fr)]">
          {snapshot ? <ExpertisePanel snapshot={snapshot} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
          <AccountActivity report={account} projects={data.projects.length} chats={data.chats.length} />
        </div>
        {account && <QuotaPanel quotas={account.quotas} />}
        </TabsContent>
        <TabsContent value="data">
          {personal
            ? (
              <SettingsSection title={t("profile.data.title")} description={t("profile.data.description")}>
                <ProfileForm initial={personal} busy={saving} submitLabel={t("profile.save")} onSubmit={save} />
              </SettingsSection>
            )
            : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        </TabsContent>
        <TabsContent value="security"><SecurityPanel /></TabsContent>
        <TabsContent value="linked"><LinkedAccounts /></TabsContent>
      </Tabs>
    </ScrollPage>
  );
}
