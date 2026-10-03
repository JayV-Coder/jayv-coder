import { useEffect, useState } from "react";
import { reportError } from "@/modules/feedback";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { acceptInvite, declineInvite, loadOrganizations, openOrganization, useOrganizations } from "@/modules/organizations";
import { EmptyText } from "@/components/atoms";
import { PageHeading, SettingsSection } from "@/components/molecules";
import { NewOrganizationDialog, YardCard } from "@/components/organisms";
import { ScrollPage } from "@/components/templates";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

/** As organizações de quem usa o app e os convites que chegaram para ele. */
export function OrganizationsPage() {
  const t = useT();
  const list = useOrganizations((state) => state.list);
  const incoming = useOrganizations((state) => state.incoming);
  const loaded = useOrganizations((state) => state.loaded);
  const [busy, setBusy] = useState<string | null>(null);
  const locale = useLocale();
  const day = (iso: string) => new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(new Date(iso));

  // Um convite pode ter chegado desde o login.
  useEffect(() => { loadOrganizations().catch(reportError); }, []);

  const answer = (id: string, action: (id: string) => Promise<void>) => {
    setBusy(id);
    action(id).catch(reportError).finally(() => setBusy(null));
  };

  return (
    <ScrollPage>
      <PageHeading eyebrow={t("header.account")} title={t("nav.organizations")} description={t("org.list.description")}>
        <NewOrganizationDialog><Button>{t("org.new.title")}</Button></NewOrganizationDialog>
      </PageHeading>

      {incoming.length > 0 && (
        <div className="mb-6">
          <SettingsSection title={t("org.invites.title")} description={t("org.invites.description")}>
            <ul className="grid gap-2">
              {incoming.map((invite) => (
                <li key={invite.id} className="flex flex-wrap items-center gap-3 rounded-md border border-border/60 px-3 py-2.5">
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium">{invite.orgName} <span className="font-mono text-xs text-muted-foreground">@{invite.orgSlug}</span></p>
                    <p className="text-xs text-muted-foreground">
                      {t("org.invites.from", { role: t(`org.role.${invite.role}` as Key), user: invite.invitedBy ? `@${invite.invitedBy}` : "—", date: day(invite.expiresAt) })}
                    </p>
                  </div>
                  <Button variant="ghost" size="sm" disabled={busy === invite.id} onClick={() => answer(invite.id, declineInvite)}>{t("org.invites.decline")}</Button>
                  <Button size="sm" disabled={busy === invite.id} onClick={() => answer(invite.id, acceptInvite)}>{t("org.invites.accept")}</Button>
                </li>
              ))}
            </ul>
          </SettingsSection>
        </div>
      )}

      {loaded && list.length === 0
        ? <EmptyText>{t("org.list.empty")}</EmptyText>
        : (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-4">
            {list.map((org) => (
              <YardCard key={org.id} onOpen={() => void openOrganization(org.id).catch(reportError)}
                actions={<Button variant="ghost" size="sm" onClick={() => void openOrganization(org.id).catch(reportError)}>{t("org.open")}</Button>}>
                <span className="text-lg font-semibold break-words">{org.name}</span>
                <span className="font-mono text-xs text-muted-foreground">@{org.slug}</span>
                <span className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  <Badge variant="outline">{t(`org.role.${org.role}` as Key)}</Badge>
                  <span>{t("org.count.members", { count: org.members })}</span>
                  <span aria-hidden="true">·</span>
                  <span>{t("org.count.repositories", { count: org.repositories })}</span>
                </span>
              </YardCard>
            ))}
          </div>
        )}
    </ScrollPage>
  );
}
