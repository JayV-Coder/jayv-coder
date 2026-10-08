import type { ReactNode } from "react";
import { switchEnvironment, useEnvironment } from "@/modules/environments";
import { useT } from "@/modules/i18n";
import { Button } from "@/components/ui/button";

/** O que depende dos dados deste computador (projetos, chats, clones, uso)
 * só existe no ambiente da organização. Fora dele, em vez do conteúdo, vem o
 * caminho para entrar nele. */
export function EnvironmentGate({ organization, children }: { organization: { id: string; name: string }; children: ReactNode }) {
  const t = useT();
  const active = useEnvironment((state) => state.active);
  const switching = useEnvironment((state) => state.switching);
  if (active === organization.id) return <>{children}</>;
  return (
    <div className="grid max-w-prose gap-3 rounded-md border border-border/60 bg-card p-4">
      <p className="text-sm text-muted-foreground">{t("environment.banner", { org: organization.name })}</p>
      <div>
        <Button size="sm" disabled={switching} onClick={() => void switchEnvironment(organization.id)}>{t("environment.switchTo", { name: organization.name })}</Button>
      </div>
    </div>
  );
}
