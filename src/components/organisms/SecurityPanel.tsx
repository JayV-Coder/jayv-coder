import { useState, type FormEvent } from "react";
import { changePassword, codeDigits, codeOk, passwordOk, sendSetPasswordCode, setFirstPassword, useAuth } from "@/modules/auth";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { FormField, PasswordRules, SettingsSection } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** A senha da conta. Quem já tem troca informando a atual; quem entrou só por
 * provedor define a primeira com o código que vai ao e-mail, para que uma
 * sessão aberta não baste para cravar uma senha na conta. */
export function SecurityPanel() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const hasPassword = useAuth((state) => state.hasPassword);
  const [current, setCurrent] = useState("");
  const [code, setCode] = useState("");
  const [codeSent, setCodeSent] = useState(false);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const mismatch = confirm.length > 0 && confirm !== password;
  const ready = passwordOk(password) && confirm === password && (hasPassword ? current.length > 0 : codeOk(code));

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

  const reset = () => {
    setCurrent("");
    setCode("");
    setCodeSent(false);
    setPassword("");
    setConfirm("");
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    void run(async () => {
      if (hasPassword) await changePassword(current, password);
      else await setFirstPassword(code, password);
      reset();
    }, hasPassword ? t("security.changed") : t("security.set"));
  };

  const sendCode = () => void run(async () => {
    await sendSetPasswordCode();
    setCodeSent(true);
  });

  return (
    <SettingsSection title={t("security.title")} description={t("security.description")}>
      {!hasPassword && !codeSent ? (
        <div className="grid gap-3">
          <p className="text-sm text-muted-foreground">{t("security.noPassword")}</p>
          <Button type="button" variant="outline" className="justify-self-start" disabled={busy || !email} onClick={sendCode}>{t("security.sendCode")}</Button>
        </div>
      ) : (
        <form onSubmit={submit} className="grid gap-4">
          {hasPassword ? (
            <FormField label={t("security.current")} htmlFor="security-current">
              <Input id="security-current" type="password" autoComplete="current-password" value={current} onChange={(event) => setCurrent(event.target.value)} />
            </FormField>
          ) : (
            <FormField label={t("security.code")} htmlFor="security-code" hint={t("security.codeSent", { email: email ?? "" })}>
              <Input id="security-code" inputMode="numeric" autoComplete="one-time-code" value={code} onChange={(event) => setCode(codeDigits(event.target.value))} />
            </FormField>
          )}
          <FormField label={t("security.new")} htmlFor="security-new">
            <Input id="security-new" type="password" autoComplete="new-password" value={password} onChange={(event) => setPassword(event.target.value)} />
          </FormField>
          <PasswordRules password={password} />
          <FormField label={t("security.confirm")} htmlFor="security-confirm" error={mismatch ? t("auth.mismatch") : null}>
            <Input id="security-confirm" type="password" autoComplete="new-password" aria-invalid={mismatch || undefined} value={confirm} onChange={(event) => setConfirm(event.target.value)} />
          </FormField>
          <div className="flex flex-wrap justify-end gap-2">
            {!hasPassword && <Button type="button" variant="ghost" disabled={busy} onClick={sendCode}>{t("security.sendCode")}</Button>}
            <Button type="submit" disabled={busy || !ready}>{hasPassword ? t("security.change") : t("security.setPassword")}</Button>
          </div>
        </form>
      )}
    </SettingsSection>
  );
}
