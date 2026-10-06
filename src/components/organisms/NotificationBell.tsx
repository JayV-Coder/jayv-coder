import { useMemo, useState } from "react";
import { BellIcon } from "lucide-react";
import { useIntentHandler } from "@/modules/commands";
import { useT } from "@/modules/i18n";
import { reportError } from "@/modules/feedback";
import { acceptInvite, declineInvite, useOrganizations } from "@/modules/organizations";
import { allNotifications, clearRead, markAllRead, openNotification, useNotifications, type AppNotification } from "@/modules/notifications";
import { NotificationItem } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { cn } from "@/lib/utils";

/** O sino da lateral e a central que ele abre: as da conta (organizações) e
 * as deste aparelho (respostas, perguntas, falhas, cotas) numa lista só, da
 * mais nova para a mais velha. O convite ainda pendente se responde ali. */
export function NotificationBell() {
  const t = useT();
  const [open, setOpen] = useState(false);
  useIntentHandler("notifications", () => setOpen(true));
  const [busy, setBusy] = useState<string | null>(null);
  const account = useNotifications((state) => state.account);
  const device = useNotifications((state) => state.device);
  const incoming = useOrganizations((state) => state.incoming);
  const list = useMemo(() => allNotifications({ account, device }), [account, device]);
  const unread = list.filter((item) => !item.read).length;
  const anyRead = list.some((item) => item.read);

  const go = (notification: AppNotification) => {
    setOpen(false);
    openNotification(notification);
  };
  const answer = async (invite: string, action: (id: string) => Promise<void>) => {
    setBusy(invite);
    try { await action(invite); } catch (error) { reportError(error); } finally { setBusy(null); }
  };
  const inviteActions = (notification: AppNotification) => {
    const invite = notification.kind === "org.invited" ? incoming.find((item) => item.id === notification.data.inviteId) : undefined;
    if (!invite) return undefined;
    return (
      <>
        <Button size="xs" disabled={busy === invite.id} onClick={() => void answer(invite.id, acceptInvite)}>{t("org.invites.accept")}</Button>
        <Button size="xs" variant="ghost" disabled={busy === invite.id} onClick={() => void answer(invite.id, declineInvite)}>{t("org.invites.decline")}</Button>
      </>
    );
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={unread > 0 ? t("notifications.unread", { count: unread }) : t("notifications.title")}
          title={t("notifications.title")}
          className={cn(
            "relative grid size-9 shrink-0 place-items-center rounded-md text-sidebar-muted transition-colors outline-none hover:bg-sidebar-accent/70 hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring",
            open && "bg-sidebar-accent text-sidebar-foreground",
          )}
        >
          <BellIcon aria-hidden="true" className="size-[18px]" />
          {unread > 0 && (
            <span aria-hidden="true" className="absolute -top-0.5 -end-0.5 grid h-4 min-w-4 place-items-center rounded-full bg-primary px-1 font-mono text-[10px] leading-none font-semibold text-primary-foreground tabular-nums ring-2 ring-sidebar">
              {unread > 99 ? "99+" : unread}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent side="right" align="start" sideOffset={10} className="flex max-h-[min(560px,calc(100vh-32px))] w-[380px] flex-col p-0">
        <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3">
          <div className="grid">
            <h2 className="text-sm font-semibold">{t("notifications.title")}</h2>
            <p className="text-caption text-muted-foreground">{unread > 0 ? t("notifications.unread", { count: unread }) : t("notifications.allRead")}</p>
          </div>
          {unread > 0 && <Button size="sm" variant="ghost" onClick={() => void markAllRead()}>{t("notifications.markAllRead")}</Button>}
        </div>
        {list.length === 0 ? (
          <div className="grid place-items-center gap-2 px-6 py-10 text-center">
            <BellIcon aria-hidden="true" className="size-6 text-muted-foreground" />
            <p className="text-sm font-medium">{t("notifications.empty.title")}</p>
            <p className="text-caption text-muted-foreground">{t("notifications.empty.description")}</p>
          </div>
        ) : (
          <ul className="grid min-h-0 gap-0.5 overflow-y-auto p-1.5">
            {list.map((notification) => (
              <NotificationItem key={notification.id} notification={notification} onOpen={() => go(notification)} actions={inviteActions(notification)} />
            ))}
          </ul>
        )}
        {anyRead && (
          <div className="flex justify-end border-t border-border px-2 py-1.5">
            <Button size="sm" variant="ghost" className="text-muted-foreground" onClick={() => void clearRead().catch(reportError)}>{t("notifications.clearRead")}</Button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
