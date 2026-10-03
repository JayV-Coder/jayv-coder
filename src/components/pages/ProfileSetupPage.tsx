import { useState } from "react";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { saveProfile, skipSetup, type AccountProfile } from "@/modules/profile";
import { BrandMark } from "@/components/atoms";
import { ProfileForm } from "@/components/organisms";
import { Button } from "@/components/ui/button";

/** O passo depois do primeiro login (por e-mail ou por provedor): o perfil
 * vem com o nome que o registro ou o provedor deram e o fuso do sistema.
 * Salvar ou pular fecham o passo de vez. */
export function ProfileSetupPage({ profile }: { profile: AccountProfile }) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const initial = { ...profile, timezone: profile.timezone ?? Intl.DateTimeFormat().resolvedOptions().timeZone };

  const run = (action: () => Promise<void>) => {
    setBusy(true);
    action().catch(reportError).finally(() => setBusy(false));
  };

  return (
    <div className="grid min-h-full place-items-center px-6 py-10">
      <div className="grid w-full max-w-2xl gap-6">
        <div className="grid justify-items-center gap-3 text-center">
          <BrandMark />
          <h1 className="text-h3 font-semibold">{t("profile.setup.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("profile.setup.description")}</p>
        </div>
        <ProfileForm initial={initial} busy={busy} submitLabel={t("profile.save")} onSubmit={(draft) => run(() => saveProfile(draft))}
          secondary={<Button type="button" variant="ghost" disabled={busy} onClick={() => run(skipSetup)}>{t("profile.skip")}</Button>} />
      </div>
    </div>
  );
}
