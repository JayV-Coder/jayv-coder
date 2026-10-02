import { useState } from "react";
import { canUnlink, linkProvider, PROVIDERS, unlinkProvider, useAuth, type Provider } from "@/modules/auth";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { PROVIDER_NAMES, ProviderIcon } from "@/components/atoms";
import { ConfirmAction, SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";

/** GitHub, GitLab e Bitbucket: vincular abre o navegador e volta pelo mesmo
 * link do login; desvincular fica travado enquanto for a única porta. */
export function LinkedAccounts() {
  const t = useT();
  const providers = useAuth((state) => state.providers);
  const hasPassword = useAuth((state) => state.hasPassword);
  const waiting = useAuth((state) => state.waitingBrowser);
  const [chosen, setChosen] = useState<Provider | null>(null);
  const [busy, setBusy] = useState(false);

  const link = (provider: Provider) => {
    setChosen(provider);
    linkProvider(provider).catch(reportError);
  };

  const unlink = (provider: Provider) => {
    setBusy(true);
    unlinkProvider(provider).catch(reportError).finally(() => setBusy(false));
  };

  return (
    <SettingsSection title={t("linked.title")} description={t("linked.description")}>
      <ul className="grid gap-2">
        {PROVIDERS.map((provider) => {
          const linked = providers.includes(provider);
          const name = PROVIDER_NAMES[provider];
          const removable = canUnlink(providers, hasPassword, provider);
          return (
            <li key={provider} className="flex items-center gap-3 rounded-md border border-border/60 px-3 py-2.5">
              <ProviderIcon provider={provider} className="size-5" />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">{name}</p>
                <p className="text-xs text-muted-foreground">{linked ? t("linked.on") : t("linked.off")}</p>
              </div>
              {linked ? (
                removable ? (
                  <ConfirmAction title={t("linked.unlink.title", { provider: name })} description={t("linked.unlink.description", { provider: name })} confirm={t("linked.unlink")} onConfirm={() => unlink(provider)}>
                    <Button type="button" variant="outline" size="sm" disabled={busy}>{t("linked.unlink")}</Button>
                  </ConfirmAction>
                ) : (
                  // O botão desativado não recebe o mouse: a dica fica no invólucro.
                  <span title={t("auth.lastIdentity")}><Button type="button" variant="outline" size="sm" disabled>{t("linked.unlink")}</Button></span>
                )
              ) : (
                <Button type="button" variant="outline" size="sm" disabled={busy} onClick={() => link(provider)}>
                  {waiting && chosen === provider ? t("auth.waitingBrowser") : t("linked.link")}
                </Button>
              )}
            </li>
          );
        })}
      </ul>
    </SettingsSection>
  );
}
