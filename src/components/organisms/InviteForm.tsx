import { useEffect, useState, type FormEvent } from "react";
import { reportError, notify } from "@/modules/feedback";
import { useT, type Key } from "@/modules/i18n";
import { findUsers, INVITE_ROLES, inviteMember, type FoundUser, type Role } from "@/modules/organizations";
import { UserAvatar } from "@/components/atoms";
import { OptionSelect } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Convidar por `@usuário` (com busca enquanto se digita) ou por e-mail. O
 * e-mail não é buscado: dizer quem tem conta seria vazar a lista. */
export function InviteForm({ orgId }: { orgId: string }) {
  const t = useT();
  const [target, setTarget] = useState("");
  const [role, setRole] = useState<Role>("member");
  const [found, setFound] = useState<FoundUser[]>([]);
  const [busy, setBusy] = useState(false);
  const isEmail = /^[^@\s]+@[^@\s]+$/.test(target.trim());

  useEffect(() => {
    const query = target.trim().replace(/^@/, "");
    if (isEmail || query.length < 2) { setFound([]); return; }
    let live = true;
    const timer = setTimeout(() => {
      findUsers(query).then((users) => { if (live) setFound(users); }).catch(() => { if (live) setFound([]); });
    }, 250);
    return () => { live = false; clearTimeout(timer); };
  }, [target, isEmail]);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!target.trim()) return;
    setBusy(true);
    try {
      await inviteMember(orgId, target, role);
      notify(t("org.invite.sent", { target: target.trim() }));
      setTarget("");
      setFound([]);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <form onSubmit={submit} className="grid gap-2">
      <div className="flex flex-wrap items-start gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Input aria-label={t("org.invite.target")} placeholder={t("org.invite.placeholder")} autoCapitalize="none" spellCheck={false}
            value={target} onChange={(event) => setTarget(event.target.value)} />
          {found.length > 0 && (
            <ul role="listbox" className="absolute inset-x-0 top-full z-10 mt-1 grid overflow-hidden rounded-md border border-border bg-popover shadow-lg">
              {found.map((user) => (
                <li key={user.userId} role="option" aria-selected={false}>
                  <button type="button" className="flex w-full items-center gap-2 px-3 py-2 text-start text-sm hover:bg-accent"
                    onClick={() => { setTarget(`@${user.username}`); setFound([]); }}>
                    <UserAvatar name={user.displayName} src={user.avatarUrl} className="size-6 text-[11px]" />
                    <span className="truncate">{user.displayName}</span>
                    <span className="font-mono text-xs text-muted-foreground">@{user.username}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="w-40">
          <OptionSelect label={t("org.invite.role")} value={role} onChange={setRole}
            options={INVITE_ROLES.map((value) => ({ value, label: t(`org.role.${value}` as Key) }))} />
        </div>
        <Button type="submit" disabled={busy || !target.trim()}>{t("org.invite.send")}</Button>
      </div>
      <p className="text-[11.5px] text-muted-foreground">{isEmail ? t("org.invite.emailNote") : t("org.invite.hint")}</p>
    </form>
  );
}
