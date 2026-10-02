import { useState, type FormEvent } from "react";
import { QRCodeSVG } from "qrcode.react";
import { cancelTotp, confirmTotp, enrollTotp, removeTotp, secretGroups, TOTP_LENGTH, totpDigits, totpOk, useAuth, type TotpEnrollment } from "@/modules/auth";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { FormField, SettingsSection } from "@/components/molecules";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** O segundo fator da conta: um app autenticador (TOTP). Ligar mostra o QR
 * code e só vale depois do primeiro código; desligar pede um código atual. */
export function TwoFactorPanel() {
  const t = useT();
  const factorId = useAuth((state) => state.totpFactorId);
  const [enrollment, setEnrollment] = useState<TotpEnrollment | null>(null);
  const [removing, setRemoving] = useState(false);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);

  const run = async (action: () => Promise<void>, done?: string) => {
    setBusy(true);
    try {
      await action();
      if (done) notify(done);
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const start = () => void run(async () => {
    setCode("");
    setEnrollment(await enrollTotp());
  });

  const cancel = () => {
    const pending = enrollment;
    setEnrollment(null);
    setRemoving(false);
    setCode("");
    if (pending) cancelTotp(pending.factorId).catch(reportError);
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!totpOk(code)) return;
    if (enrollment) {
      void run(async () => {
        await confirmTotp(enrollment.factorId, code);
        setEnrollment(null);
        setCode("");
      }, t("twoFactor.enabled"));
    } else {
      void run(async () => {
        await removeTotp(code);
        setRemoving(false);
        setCode("");
      }, t("twoFactor.removed"));
    }
  };

  const codeField = (
    <FormField label={t("twoFactor.code")} htmlFor="two-factor-code" hint={removing ? t("twoFactor.removeHint") : undefined}>
      <Input id="two-factor-code" inputMode="numeric" autoComplete="one-time-code" autoFocus maxLength={TOTP_LENGTH} className="max-w-40 font-mono tracking-[0.3em]" value={code} onChange={(event) => setCode(totpDigits(event.target.value))} />
    </FormField>
  );

  return (
    <SettingsSection
      title={t("twoFactor.title")}
      description={t("twoFactor.description")}
      action={<Badge variant={factorId ? "success" : "secondary"}>{factorId ? t("twoFactor.badge.on") : t("twoFactor.badge.off")}</Badge>}
    >
      {enrollment ? (
        <form onSubmit={submit} className="grid gap-4">
          <p className="text-sm text-muted-foreground">{t("twoFactor.scan")}</p>
          <div className="flex flex-wrap items-start gap-5">
            {/* Fundo branco fixo: o leitor de QR não lê o código no tema escuro. */}
            <div className="rounded-md bg-white p-3">
              <QRCodeSVG value={enrollment.uri} size={168} marginSize={0} />
            </div>
            <div className="grid min-w-0 flex-1 gap-4">
              <div className="grid gap-1.5">
                <p className="text-xs text-muted-foreground">{t("twoFactor.secret")}</p>
                <code className="select-all break-all rounded-md border border-border/60 px-3 py-2 font-mono text-sm">{secretGroups(enrollment.secret)}</code>
              </div>
              {codeField}
            </div>
          </div>
          <p className="text-xs text-muted-foreground">{t("twoFactor.warning")}</p>
          <div className="flex flex-wrap justify-end gap-2">
            <Button type="button" variant="ghost" disabled={busy} onClick={cancel}>{t("common.cancel")}</Button>
            <Button type="submit" disabled={busy || !totpOk(code)}>{t("twoFactor.confirm")}</Button>
          </div>
        </form>
      ) : factorId ? (
        removing ? (
          <form onSubmit={submit} className="grid gap-4">
            {codeField}
            <div className="flex flex-wrap justify-end gap-2">
              <Button type="button" variant="ghost" disabled={busy} onClick={cancel}>{t("common.cancel")}</Button>
              <Button type="submit" variant="destructive" disabled={busy || !totpOk(code)}>{t("twoFactor.remove")}</Button>
            </div>
          </form>
        ) : (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="text-sm text-muted-foreground">{t("twoFactor.on")}</p>
            <Button type="button" variant="outline" disabled={busy} onClick={() => { setCode(""); setRemoving(true); }}>{t("twoFactor.remove")}</Button>
          </div>
        )
      ) : (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <p className="text-sm text-muted-foreground">{t("twoFactor.off")}</p>
          <Button type="button" disabled={busy} onClick={start}>{t("twoFactor.enable")}</Button>
        </div>
      )}
    </SettingsSection>
  );
}
