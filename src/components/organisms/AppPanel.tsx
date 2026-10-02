import type { CoreSettings, CoreSnapshot } from "@/modules/core";
import { useT } from "@/modules/i18n";
import { updateCore } from "@/modules/settings";
import { checkForUpdate, isUpdateBusy, useUpdate } from "@/modules/updates";
import { FormField, LanguageSelect, SettingsSection, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

/** Um padrão por linha: é assim que se escreve, e é assim que se lê. */
const lines = (text: string) => text.split("\n");

/** O aplicativo: o idioma da tela e das respostas, o que nunca sai da máquina
 * e a versão instalada. */
export function AppPanel({ core, snapshot }: { core: CoreSettings; snapshot: CoreSnapshot }) {
  const t = useT();
  // O botão abre a mesma janela da abertura do app; enquanto ela trabalha,
  // clicar de novo só a traz de volta.
  const checking = useUpdate((state) => isUpdateBusy(state.phase));
  const check = () => checkForUpdate(true);

  return (
    <div className="grid gap-5">
      <SettingsSection title={t("app.section.language")} description={t("app.section.language.description")}>
        <LanguageSelect />
      </SettingsSection>

      <SettingsSection title={t("app.section.privacy")} description={t("app.section.privacy.description")}>
        <div className="grid gap-4">
          <ToggleRow id="app-redact" label={t("app.redact")} hint={t("app.redact.hint")} checked={core.privacy.redactSecrets}
            onChange={(redactSecrets) => updateCore({ privacy: { ...core.privacy, redactSecrets } })} />
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label={t("app.deny")} htmlFor="app-deny" hint={t("app.deny.hint")}>
              <Textarea id="app-deny" rows={6} spellCheck={false} className="font-mono text-[12.5px]" value={core.privacy.deny.join("\n")}
                onChange={(event) => updateCore({ privacy: { ...core.privacy, deny: lines(event.target.value) } })} />
            </FormField>
            <FormField label={t("app.localOnly")} htmlFor="app-local-only" hint={t("app.localOnly.hint")}>
              <Textarea id="app-local-only" rows={6} spellCheck={false} className="font-mono text-[12.5px]" value={core.privacy.localOnly.join("\n")}
                onChange={(event) => updateCore({ privacy: { ...core.privacy, localOnly: lines(event.target.value) } })} />
            </FormField>
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={t("app.section.version")} description={t("app.version", { version: snapshot.version })}
        action={<Button variant="outline" disabled={checking} onClick={() => void check()}>{t(checking ? "app.checking" : "app.checkUpdates")}</Button>}>
        <p className="text-[12.5px] text-muted-foreground">{t("app.updates.hint")}</p>
      </SettingsSection>
    </div>
  );
}
