import { useAuth } from "@/modules/auth";
import { useT } from "@/modules/i18n";
import { useSettings } from "@/modules/settings";
import { useUsage } from "@/modules/usage";
import { useWorkspace } from "@/modules/workspace";
import { LoadingNote } from "@/components/atoms";
import { AccountActivity, ExpertisePanel, ProfileCard, QuotaPanel } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";

/** A conta de quem usa o app: quem é, o nível com que o Jev o trata, o que
 * fez no último mês e quanto resta dos planos dos agentes. */
export function ProfilePage() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const profile = useAuth((state) => state.profile);
  const snapshot = useSettings((state) => state.coreSnapshot);
  const account = useUsage((state) => state.account);
  const { data } = useWorkspace();

  return (
    <ScrollPage>
      <ProfileCard email={email} profile={profile} expertise={snapshot?.expertise ?? null} />
      <div className="mb-6 grid items-start gap-5 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,1fr)]">
        {snapshot ? <ExpertisePanel snapshot={snapshot} /> : <LoadingNote>{t("settings.loading")}</LoadingNote>}
        <AccountActivity report={account} projects={data.projects.length} chats={data.chats.length} />
      </div>
      {account && <QuotaPanel quotas={account.quotas} />}
    </ScrollPage>
  );
}
