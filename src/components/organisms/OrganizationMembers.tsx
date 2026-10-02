import { useState } from "react";
import { reportError } from "@/modules/feedback";
import { useLocale, useT, type Key } from "@/modules/i18n";
import { can, removeMember, revokeInvite, ROLES, setMemberRole, type OrganizationDetail, type Role } from "@/modules/organizations";
import { useProfile } from "@/modules/profile";
import { UserAvatar } from "@/components/atoms";
import { ConfirmAction, OptionSelect, SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { InviteForm } from "./InviteForm";

/** Os membros e, para quem gere, o convite e os convites pendentes. */
export function OrganizationMembers({ detail, role }: { detail: OrganizationDetail; role: Role }) {
  const t = useT();
  const locale = useLocale();
  const me = useProfile((state) => state.profile?.username);
  const [busy, setBusy] = useState(false);
  const day = (iso: string) => new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(new Date(iso));
  const roleName = (value: Role) => t(`org.role.${value}` as Key);

  const run = (action: () => Promise<void>) => {
    setBusy(true);
    action().catch(reportError).finally(() => setBusy(false));
  };

  return (
    <div className="grid gap-5">
      {can.manage(role) && (
        <SettingsSection title={t("org.invite.title")} description={t("org.invite.description")}>
          <InviteForm orgId={detail.id} />
          {detail.invites.length > 0 && (
            <ul className="grid gap-2">
              {detail.invites.map((invite) => (
                <li key={invite.id} className="flex flex-wrap items-center gap-3 rounded-md border border-dashed border-border/70 px-3 py-2">
                  <span className="min-w-0 flex-1 truncate text-sm">{invite.username ? `@${invite.username}` : invite.email}</span>
                  <Badge variant="outline">{roleName(invite.role)}</Badge>
                  <span className="text-xs text-muted-foreground">{t("org.invite.expires", { date: day(invite.expiresAt) })}</span>
                  <Button variant="ghost" size="sm" disabled={busy} onClick={() => run(() => revokeInvite(invite.id))}>{t("org.invite.revoke")}</Button>
                </li>
              ))}
            </ul>
          )}
        </SettingsSection>
      )}
      <SettingsSection title={t("org.members.title")} description={t("org.count.members", { count: detail.members.length })}>
        <ul className="grid gap-2">
          {detail.members.map((member) => {
            const self = member.username === me;
            return (
              <li key={member.userId} className="flex flex-wrap items-center gap-3 rounded-md border border-border/60 px-3 py-2.5">
                <UserAvatar name={member.displayName} src={member.avatarUrl} className="size-8 text-sm" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm font-medium">{member.displayName}{self && <span className="ms-2 text-xs text-muted-foreground">{t("org.members.you")}</span>}</p>
                  <p className="truncate font-mono text-xs text-muted-foreground">@{member.username}</p>
                </div>
                {can.changeRoles(role) && !self ? (
                  <div className="w-40">
                    <OptionSelect label={t("org.members.role", { user: member.username })} value={member.role} disabled={busy}
                      options={ROLES.map((value) => ({ value, label: roleName(value) }))}
                      onChange={(next) => run(() => setMemberRole(detail.id, member.userId, next))} />
                  </div>
                ) : <Badge variant="outline">{roleName(member.role)}</Badge>}
                {can.remove(role, member.role) && !self && (
                  <ConfirmAction title={t("org.members.remove.title")} description={t("org.members.remove.description", { user: member.username })}
                    confirm={t("org.members.remove")} onConfirm={() => run(() => removeMember(detail.id, member.userId))}>
                    <Button variant="ghost" size="sm" disabled={busy} className="text-muted-foreground hover:text-destructive">{t("org.members.remove")}</Button>
                  </ConfirmAction>
                )}
              </li>
            );
          })}
        </ul>
      </SettingsSection>
    </div>
  );
}
