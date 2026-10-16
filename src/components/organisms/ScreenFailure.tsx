import { TriangleAlertIcon } from "lucide-react";
import { useT } from "@/modules/i18n";
import { Button } from "@/components/ui/button";

/** O lugar de uma tela que falhou ao desenhar: o motivo técnico e o caminho de
 * volta. A lateral e o resto do app continuam de pé — antes, a falha deixava a
 * janela inteira em branco. */
export function ScreenFailure({ error, onRetry }: { error: Error; onRetry: () => void }) {
  const t = useT();
  return (
    <div className="h-full overflow-y-auto px-5">
      <div role="alert" className="mx-auto mt-16 grid max-w-md justify-items-center gap-3 text-center">
        <span className="grid size-10 place-items-center rounded-full bg-muted text-muted-foreground"><TriangleAlertIcon aria-hidden="true" className="size-5" /></span>
        <h2 className="text-h3 font-semibold">{t("error.screen.title")}</h2>
        <p className="font-mono text-xs break-all text-muted-foreground">{error.message}</p>
        <Button onClick={onRetry}>{t("error.screen.retry")}</Button>
      </div>
    </div>
  );
}
