import { useState } from "react";
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

/** A conta de quem usa o app: quem é, os dados pessoais, a senha e as contas
 * vinculadas, o nível com que o Jev o trata, o que fez no último mês e quanto
 * resta dos planos dos agentes. */
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
      {personal && (
        <div className="mb-5">
          <SettingsSection title={t("profile.data.title")} description={t("profile.data.description")}>
            <ProfileForm initial={personal} busy={saving} submitLabel={t("profile.save")} onSubmit={save} />
          </SettingsSection>
        </div>
      )}
      <div className="mb-5 grid items-start gap-5 lg:grid-cols-2">
        <SecurityPanel />
        <LinkedAccounts />
      </div>
      <div className="mb-6 grid items-start gap-5 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,1fr)]">
        {snapshot ? <ExpertisePanel snapshot={snapshot} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        <AccountActivity report={account} projects={data.projects.length} chats={data.chats.length} />
      </div>
      {account && <QuotaPanel quotas={account.quotas} />}
    </ScrollPage>
  );
}
