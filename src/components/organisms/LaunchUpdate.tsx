import { LoaderCircleIcon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { skipLaunchUpdate, useUpdate } from "@/modules/updates";
import { Button } from "@/components/ui/button";

/** A tela de abertura enquanto o app confere se há versão nova: sem rede ou
 * com pressa, "Pular" abre o app na hora. Com versão nova, a janela de
 * progresso da atualização assume. */
export function LaunchUpdate() {
  const t = useT();
  const launching = useUpdate((state) => state.launching);
  if (!launching) return null;
  return (
    <div role="status" aria-live="polite" className="fixed inset-0 z-[60] grid place-items-center bg-background">
      <div className="grid justify-items-center gap-3 px-6 text-center">
        <LoaderCircleIcon aria-hidden="true" className="size-6 animate-spin text-muted-foreground motion-reduce:animate-none" />
        <p className="text-sm font-medium">{t("update.launch.checking")}</p>
        <p className="max-w-xs text-xs text-muted-foreground">{t("update.launch.note")}</p>
        <Button size="sm" variant="ghost" onClick={skipLaunchUpdate}>{t("update.launch.skip")}</Button>
      </div>
    </div>
  );
}
