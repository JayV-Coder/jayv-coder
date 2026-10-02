import { useState, type FormEvent } from "react";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { addRepository, can, parseRepoUrl, removeRepository, type OrganizationDetail, type Role } from "@/modules/organizations";
import { PROVIDER_NAMES, ProviderIcon } from "@/components/atoms";
import { ConfirmAction, SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Os repositórios da organização. Um projeto local entra nela quando um
 * remote do git da pasta casa com um deles e o dono do projeto é membro. */
export function OrganizationRepositories({ detail, role }: { detail: OrganizationDetail; role: Role }) {
  const t = useT();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const parsed = parseRepoUrl(url);
  const manages = can.manage(role);

  const run = (action: () => Promise<void>) => {
    setBusy(true);
    return action().catch(reportError).finally(() => setBusy(false));
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!parsed) return;
    void run(async () => { await addRepository(detail.id, parsed.provider, parsed.path); setUrl(""); });
  };

  return (
    <SettingsSection title={t("org.repos.title")} description={t("org.repos.description")}>
      {manages && (
        <form onSubmit={submit} className="grid gap-2">
          <div className="flex flex-wrap gap-2">
            <Input aria-label={t("org.repos.url")} placeholder="https://github.com/acme/api" spellCheck={false} autoCapitalize="none"
              className="min-w-[240px] flex-1 font-mono text-sm" aria-invalid={url.trim() && !parsed ? true : undefined} value={url} onChange={(event) => setUrl(event.target.value)} />
            <Button type="submit" disabled={busy || !parsed}>{t("org.repos.add")}</Button>
          </div>
          <p className={url.trim() && !parsed ? "text-[11.5px] text-destructive" : "text-[11.5px] text-muted-foreground"}>
            {parsed
              ? <span className="inline-flex items-center gap-1.5"><ProviderIcon provider={parsed.provider} className="size-3.5" />{PROVIDER_NAMES[parsed.provider]} · <span className="font-mono">{parsed.path}</span></span>
              : url.trim() ? t("org.repos.invalid") : t("org.repos.hint")}
          </p>
        </form>
      )}
      {detail.repositories.length === 0
        ? <p className="text-sm text-muted-foreground">{t("org.repos.empty")}</p>
        : (
          <ul className="grid gap-2">
            {detail.repositories.map((repository) => (
              <li key={repository.id} className="flex items-center gap-3 rounded-md border border-border/60 px-3 py-2.5">
                <ProviderIcon provider={repository.provider} className="size-5" />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-sm">{repository.path}</p>
                  <p className="text-xs text-muted-foreground">{PROVIDER_NAMES[repository.provider]}</p>
                </div>
                {manages && (
                  <ConfirmAction title={t("org.repos.remove.title")} description={t("org.repos.remove.description", { repo: repository.repoKey })}
                    confirm={t("org.repos.remove")} onConfirm={() => void run(() => removeRepository(repository.id))}>
                    <Button variant="ghost" size="sm" disabled={busy} className="text-muted-foreground hover:text-destructive">{t("org.repos.remove")}</Button>
                  </ConfirmAction>
                )}
              </li>
            ))}
          </ul>
        )}
    </SettingsSection>
  );
}
