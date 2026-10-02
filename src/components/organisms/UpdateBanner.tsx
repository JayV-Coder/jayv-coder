import { ArrowUpCircleIcon, XIcon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { dismissUpdate, installUpdate, showUpdate, useUpdate } from "@/modules/updates";
import { Button } from "@/components/ui/button";

/** O aviso no topo de qualquer tela quando há versão nova esperando: o que
 * muda (a janela com as notas) e atualizar agora. Fechar esconde só esta
 * versão; a notificação continua no sino. */
export function UpdateBanner() {
  const t = useT();
  const version = useUpdate((state) => (state.phase === "available" && state.next !== state.dismissed ? state.next : null));
  if (!version) return null;
  return (
    <div role="status" className="flex flex-none items-center gap-3 border-b border-primary/25 bg-primary/10 px-4 py-2 text-sm text-foreground">
      <ArrowUpCircleIcon aria-hidden="true" className="size-4 shrink-0 text-primary" />
      <p className="min-w-0 flex-1 truncate">
        <span className="font-medium">{t("update.banner.title", { version })}</span>
        <span className="text-muted-foreground"> · {t("update.banner.description")}</span>
      </p>
      <Button size="xs" variant="ghost" onClick={showUpdate}>{t("update.banner.details")}</Button>
      <Button size="xs" onClick={() => void installUpdate()}>{t("update.install")}</Button>
      <Button size="icon-xs" variant="ghost" aria-label={t("update.banner.dismiss")} title={t("update.banner.dismiss")} onClick={dismissUpdate}>
        <XIcon aria-hidden="true" />
      </Button>
    </div>
  );
}
