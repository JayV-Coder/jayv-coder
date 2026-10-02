import { useState, type FormEvent } from "react";
import { finishRecovery, passwordOk, signOut } from "@/modules/auth";
import { notify, reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { BrandMark } from "@/components/atoms";
import { FormField, PasswordRules } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Aberta pelo link de recuperação: a sessão já existe, falta a senha nova.
 * Cancelar sai da conta, para o link não virar uma porta aberta. */
export function NewPasswordPage() {
  const t = useT();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const mismatch = confirm.length > 0 && confirm !== password;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    try {
      await finishRecovery(password);
      notify(t("auth.newPassword.saved"));
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid min-h-screen place-items-center px-6">
      <form onSubmit={submit} className="grid w-full max-w-sm gap-4">
        <div className="grid justify-items-center gap-3 text-center">
          <BrandMark />
          <h1 className="text-h3 font-semibold tracking-tight">{t("auth.newPassword.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("auth.newPassword.description")}</p>
        </div>
        <FormField label={t("security.new")} htmlFor="recover-password">
          <Input id="recover-password" type="password" autoComplete="new-password" required value={password} onChange={(event) => setPassword(event.target.value)} />
        </FormField>
        <PasswordRules password={password} />
        <FormField label={t("security.confirm")} htmlFor="recover-confirm" error={mismatch ? t("auth.mismatch") : null}>
          <Input id="recover-confirm" type="password" autoComplete="new-password" required aria-invalid={mismatch || undefined} value={confirm} onChange={(event) => setConfirm(event.target.value)} />
        </FormField>
        <Button type="submit" disabled={busy || !passwordOk(password) || confirm !== password}>{t("auth.newPassword.save")}</Button>
        <Button type="button" variant="ghost" onClick={() => void signOut()}>{t("common.cancel")}</Button>
      </form>
    </div>
  );
}
