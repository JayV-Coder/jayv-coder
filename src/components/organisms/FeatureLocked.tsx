import { LockIcon } from "lucide-react";
import type { FeatureKey } from "@/modules/plans";
import { useT, type Key } from "@/modules/i18n";
import { navigate } from "@/modules/navigation";
import { Button } from "@/components/ui/button";

/** O lugar de uma tela cujo recurso não está no plano de quem usa (ou que o
 * admin desligou): o que ele faz e o caminho para os planos. */
export function FeatureLocked({ feature }: { feature: FeatureKey }) {
  const t = useT();
  return (
    <div className="h-full overflow-y-auto px-5">
      <div className="mx-auto mt-16 grid max-w-md justify-items-center gap-3 text-center">
        <span className="grid size-10 place-items-center rounded-full bg-muted text-muted-foreground"><LockIcon aria-hidden="true" className="size-5" /></span>
        <h2 className="text-h3 font-semibold">{t(`feature.${feature}.title` as Key)}</h2>
        <p className="text-sm text-muted-foreground">{t(`feature.${feature}.detail` as Key)}</p>
        <p className="text-sm text-muted-foreground">{t("plans.locked")}</p>
        <Button onClick={() => navigate("plans")}>{t("plans.see")}</Button>
      </div>
    </div>
  );
}
