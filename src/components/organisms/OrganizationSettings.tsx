import { useState, type FormEvent } from "react";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { can, deleteOrganization, leaveOrganization, renameOrganization, type Organization } from "@/modules/organizations";
import { ConfirmAction, FormField, SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Renomear, sair e excluir. Excluir pede o slug digitado: apaga membros,
 * convites e repositórios de uma vez. */
export function OrganizationSettings({ organization }: { organization: Organization }) {
  const t = useT();
  const [name, setName] = useState(organization.name);
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);

  const run = (action: () => Promise<void>, done?: string) => {
    setBusy(true);
    action().then(() => { if (done) notify(done); }).catch(reportError).finally(() => setBusy(false));
  };

  const rename = (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || name.trim() === organization.name) return;
    run(() => renameOrganization(organization.id, name), t("org.settings.renamed"));
  };

  return (
    <div className="grid gap-5">
      {can.manage(organization.role) && (
        <SettingsSection title={t("org.settings.rename")}>
          <form onSubmit={rename} className="flex flex-wrap items-end gap-2">
            <FormField label={t("org.field.name")} htmlFor="org-rename" className="min-w-[240px] flex-1">
              <Input id="org-rename" maxLength={80} value={name} onChange={(event) => setName(event.target.value)} />
            </FormField>
            <Button type="submit" disabled={busy || !name.trim() || name.trim() === organization.name}>{t("profile.save")}</Button>
          </form>
        </SettingsSection>
      )}
      <SettingsSection title={t("org.settings.leave")} description={t("org.settings.leave.description")}>
        <ConfirmAction title={t("org.settings.leave")} description={t("org.settings.leave.confirm", { name: organization.name })} confirm={t("org.settings.leave")}
          onConfirm={() => run(() => leaveOrganization(organization.id))}>
          <Button variant="outline" className="justify-self-start" disabled={busy}>{t("org.settings.leave")}</Button>
        </ConfirmAction>
      </SettingsSection>
      {can.delete(organization.role) && (
        <SettingsSection title={t("org.settings.delete")} description={t("org.settings.delete.description")}>
          <div className="flex flex-wrap items-end gap-2">
            <FormField label={t("org.settings.delete.type", { slug: organization.slug })} htmlFor="org-delete" className="min-w-[240px] flex-1">
              <Input id="org-delete" spellCheck={false} autoCapitalize="none" value={confirm} onChange={(event) => setConfirm(event.target.value)} />
            </FormField>
            <Button variant="destructive" disabled={busy || confirm !== organization.slug} onClick={() => run(() => deleteOrganization(organization.id))}>
              {t("org.settings.delete")}
            </Button>
          </div>
        </SettingsSection>
      )}
    </div>
  );
}
