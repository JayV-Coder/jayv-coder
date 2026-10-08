import { LoaderCircleIcon } from "lucide-react";
import { useEnvironment } from "@/modules/environments";
import { useT } from "@/modules/i18n";

/** O carregamento da troca de ambiente: da escolha até os dados do ambiente
 * novo chegarem, sem consultar atualização no meio. */
export function EnvironmentLoading() {
  const t = useT();
  const switching = useEnvironment((state) => state.switching);
  if (!switching) return null;
  return (
    <div role="status" aria-live="polite" className="fixed inset-0 z-[60] grid place-items-center bg-background">
      <div className="grid justify-items-center gap-3 px-6 text-center">
        <LoaderCircleIcon aria-hidden="true" className="size-6 animate-spin text-muted-foreground motion-reduce:animate-none" />
        <p className="text-sm font-medium">{t("environment.switching")}</p>
      </div>
    </div>
  );
}
