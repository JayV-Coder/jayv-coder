import type { CoreSettings, CoreSnapshot } from "@/modules/core";
import { formatClock, useT } from "@/modules/i18n";
import { showChanges } from "@/modules/changelog";
import { updateCore } from "@/modules/settings";
import { locks, SENSITIVE_PATTERNS, toggleState, useEntitlements } from "@/modules/plans";
import { checkForUpdate, isUpdateBusy, setInstallOnLaunch, useUpdate } from "@/modules/updates";
import { FormField, LanguageSelect, SettingsSection, ThemeSelect, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

/** Um padrão por linha: é assim que se escreve, e é assim que se lê. */
const lines = (text: string) => text.split("\n");

/** O aplicativo: o idioma da tela e das respostas, o tema, o que nunca sai da máquina
 * e a versão instalada. */
export function AppPanel({ core, snapshot }: { core: CoreSettings; snapshot: CoreSnapshot }) {
  const t = useT();
  // O botão abre a mesma janela da abertura do app; enquanto ela trabalha,
  // clicar de novo só a traz de volta.
  const checking = useUpdate((state) => isUpdateBusy(state.phase));
  const check = () => checkForUpdate(true);
  const checkedAt = useUpdate((state) => state.checkedAt);
  const installOnLaunch = useUpdate((state) => state.installOnLaunch);
  const entitlements = useEntitlements();
  // A redação de segredos e os arquivos sensíveis são núcleo: ligados, sem
  // interruptor, com a lista padrão sempre por baixo do que quem usa escreve.
  const redact = toggleState(entitlements, "secretRedaction", core.privacy.redactSecrets);
  const sensitive = locks(entitlements, "sensitiveFiles");

  return (
    <div className="grid gap-5">
      <SettingsSection title={t("app.section.language")} description={t("app.section.language.description")}>
        <LanguageSelect />
      </SettingsSection>

      <SettingsSection title={t("app.section.theme")} description={t("app.section.theme.description")}>
        <ThemeSelect />
      </SettingsSection>

      <SettingsSection title={t("app.section.privacy")} description={t("app.section.privacy.description")}>
        <div className="grid gap-4">
          <ToggleRow id="app-redact" label={t("app.redact")} hint={redact.reason === "required" ? `${t("plans.required")} · ${t("app.redact.hint")}` : t("app.redact.hint")}
            checked={redact.checked} disabled={redact.disabled}
            onChange={(redactSecrets) => updateCore({ privacy: { ...core.privacy, redactSecrets } })} />
          <div className="grid gap-4 sm:grid-cols-2">
            <FormField label={t("app.deny")} htmlFor="app-deny"
              hint={`${t("app.deny.hint")} ${sensitive ? `${t("plans.sensitiveAlways", { patterns: SENSITIVE_PATTERNS.join(", ") })} ` : ""}${t("app.deny.agents")}`}>
              <Textarea id="app-deny" rows={6} spellCheck={false} className="font-mono text-xs" value={core.privacy.deny.join("\n")}
                onChange={(event) => updateCore({ privacy: { ...core.privacy, deny: lines(event.target.value) } })} />
            </FormField>
            <FormField label={t("app.localOnly")} htmlFor="app-local-only" hint={t("app.localOnly.hint")}>
              <Textarea id="app-local-only" rows={6} spellCheck={false} className="font-mono text-xs" value={core.privacy.localOnly.join("\n")}
                onChange={(event) => updateCore({ privacy: { ...core.privacy, localOnly: lines(event.target.value) } })} />
            </FormField>
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={t("app.section.version")} description={t("app.version", { version: snapshot.version })}
        action={(
          <div className="flex flex-wrap gap-2">
            <Button variant="ghost" onClick={() => void showChanges()}>{t("app.whatsNew")}</Button>
            <Button variant="outline" disabled={checking} onClick={() => void check()}>{t(checking ? "app.checking" : "app.checkUpdates")}</Button>
          </div>
        )}>
        <div className="grid gap-1 text-xs text-muted-foreground">
          <p>{t("app.updates.auto")}</p>
          <ToggleRow id="update-on-launch" label={t("app.updates.launch")} hint={t("app.updates.launch.hint")} checked={installOnLaunch} onChange={setInstallOnLaunch} className="mb-2 text-foreground" />
          <p>{checkedAt ? t("app.updates.checkedAt", { at: formatClock(checkedAt) }) : t("app.updates.never")}</p>
        </div>
      </SettingsSection>
    </div>
  );
}
