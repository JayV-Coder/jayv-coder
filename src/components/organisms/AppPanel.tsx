import type { CoreSettings, CoreSnapshot } from "@/modules/core";
import { formatClock, useT } from "@/modules/i18n";
import { showChanges } from "@/modules/changelog";
import { updateCore, updatePrefs, useAppPrefs } from "@/modules/settings";
import { locks, SENSITIVE_PATTERNS, toggleState, useEntitlements } from "@/modules/plans";
import { resetTours, startTour, TOURS, useTutorial } from "@/modules/tutorial";
import { checkForUpdate, isUpdateBusy, useUpdate } from "@/modules/updates";
import { FormField, LanguageSelect, SettingsSection, ThemeSelect, ToggleRow } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

/** Um padrão por linha: é assim que se escreve, e é assim que se lê. */
const lines = (text: string) => text.split("\n");

/** O aplicativo: o idioma da tela e das respostas, o tema, o que nunca sai da máquina
 * e a versão instalada. Toda escolha daqui espera o Salvar da página; os
 * botões (rever um tutorial, procurar atualização) são ações e agem na hora. */
export function AppPanel({ core, snapshot }: { core: CoreSettings; snapshot: CoreSnapshot }) {
  const t = useT();
  // O botão abre a mesma janela da abertura do app; enquanto ela trabalha,
  // clicar de novo só a traz de volta.
  const checking = useUpdate((state) => isUpdateBusy(state.phase));
  const check = () => checkForUpdate(true);
  const checkedAt = useUpdate((state) => state.checkedAt);
  const prefs = useAppPrefs();
  const tutorial = useTutorial();
  const entitlements = useEntitlements();
  // A redação de segredos e os arquivos sensíveis são núcleo: ligados, sem
  // interruptor, com a lista padrão sempre por baixo do que quem usa escreve.
  const redact = toggleState(entitlements, "secretRedaction", core.privacy.redactSecrets);
  const sensitive = locks(entitlements, "sensitiveFiles");

  return (
    <div className="grid gap-5">
      <SettingsSection title={t("app.section.language")} description={t("app.section.language.description")}>
        <LanguageSelect value={prefs.locale} onChange={(locale) => updatePrefs({ locale })} />
      </SettingsSection>

      <SettingsSection title={t("app.section.theme")} description={t("app.section.theme.description")}>
        <ThemeSelect value={prefs.theme} onChange={(theme) => updatePrefs({ theme })} />
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

      <SettingsSection title={t("tutorial.section.title")} description={t("tutorial.section.description")}
        action={<Button variant="ghost" onClick={resetTours}>{t("tutorial.reset")}</Button>}>
        <div className="grid gap-3">
          <ToggleRow id="tutorial-auto" label={t("tutorial.auto")} hint={t("tutorial.auto.hint")} checked={prefs.tutorialAuto} onChange={(tutorialAuto) => updatePrefs({ tutorialAuto })} />
          <ul className="grid gap-1.5">
            {TOURS.map((tour) => (
              <li key={tour.id} className="flex items-center justify-between gap-3 rounded-lg border border-border/60 px-3.5 py-2">
                <span className="grid text-sm">
                  <span className="font-medium">{t(`tutorial.tour.${tour.id}` as never)}</span>
                  <span className="text-xs text-muted-foreground">{t("tutorial.steps", { count: tour.steps.length })}{tutorial.seen.includes(tour.id) ? ` · ${t("tutorial.seen")}` : ""}</span>
                </span>
                <Button size="sm" variant="outline" onClick={() => startTour(tour.id)}>{t("tutorial.start")}</Button>
              </li>
            ))}
          </ul>
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
          <ToggleRow id="update-on-launch" label={t("app.updates.launch")} hint={t("app.updates.launch.hint")} checked={prefs.installOnLaunch} onChange={(installOnLaunch) => updatePrefs({ installOnLaunch })} className="mb-2 text-foreground" />
          <p>{checkedAt ? t("app.updates.checkedAt", { at: formatClock(checkedAt) }) : t("app.updates.never")}</p>
        </div>
      </SettingsSection>
    </div>
  );
}
