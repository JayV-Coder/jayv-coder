import { useLocale, useT } from "@/modules/i18n";
import { describe, targetOf, timeAgo, toneOf, type AppNotification } from "@/modules/notifications";
import { AspectDot } from "@/components/atoms";
import { cn } from "@/lib/utils";

/** Uma linha da central: a cor do tipo, a frase, a linha de contexto e quanto
 * tempo faz. A não lida tem o texto cheio e o ponto de destaque à direita. */
export function NotificationItem({ notification, onOpen, actions }: { notification: AppNotification; onOpen: () => void; actions?: React.ReactNode }) {
  const t = useT();
  const locale = useLocale();
  const { title, detail } = describe(notification, t);
  const tone = toneOf(notification.kind);
  const clickable = targetOf(notification) !== null || !notification.read;
  return (
    <li className={cn("group relative grid grid-cols-[7px_minmax(0,1fr)] gap-x-3 rounded-md ps-3 pe-6 py-2.5 transition-colors", clickable && "hover:bg-muted/60")}>
      <AspectDot aspect={tone === "info" ? null : tone} className={cn("mt-1.5", tone === "info" && "bg-info")} />
      <div className="grid min-w-0 gap-0.5">
        <button
          type="button"
          onClick={onOpen}
          disabled={!clickable}
          className="text-start text-[13px] leading-snug outline-none after:absolute after:inset-0 after:rounded-md focus-visible:after:ring-2 focus-visible:after:ring-ring disabled:cursor-default"
        >
          <span className={cn(notification.read ? "text-muted-foreground" : "font-medium text-foreground")}>{title}</span>
        </button>
        <p className="flex min-w-0 items-center gap-1.5 text-caption text-muted-foreground">
          {detail && <span className="truncate">{detail}</span>}
          {detail && <span aria-hidden="true">·</span>}
          <time dateTime={notification.createdAt} className="shrink-0 tabular-nums">{timeAgo(notification.createdAt, locale)}</time>
        </p>
        {actions && <div className="relative z-10 mt-1.5 flex gap-1.5">{actions}</div>}
      </div>
      {!notification.read && <span aria-label={t("notifications.unreadMark")} className="absolute end-3 top-3.5 size-1.5 rounded-full bg-primary" />}
    </li>
  );
}
