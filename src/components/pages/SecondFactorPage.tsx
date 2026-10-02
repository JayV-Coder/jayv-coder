import { useState, type FormEvent } from "react";
import { signOut, TOTP_LENGTH, totpDigits, totpOk, useAuth, verifySecondFactor } from "@/modules/auth";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { BrandMark } from "@/components/atoms";
import { FormField } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** Depois da senha ou do provedor, quando a conta tem app autenticador: o
 * app só abre com o código. Sair volta ao login, para a sessão pela metade
 * não ficar guardada. */
export function SecondFactorPage() {
  const t = useT();
  const email = useAuth((state) => state.email);
  const [code, setCode] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!totpOk(code)) return;
    setBusy(true);
    try {
      await verifySecondFactor(code);
    } catch (error) {
      setCode("");
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
          <h1 className="text-h3 font-semibold tracking-tight">{t("auth.secondFactor.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("auth.secondFactor.description")}</p>
        </div>
        <FormField label={t("auth.secondFactor.code")} htmlFor="second-factor-code" hint={email ?? undefined}>
          <Input id="second-factor-code" inputMode="numeric" autoComplete="one-time-code" autoFocus maxLength={TOTP_LENGTH} className="text-center font-mono tracking-[0.4em]" value={code} onChange={(event) => setCode(totpDigits(event.target.value))} />
        </FormField>
        <Button type="submit" disabled={busy || !totpOk(code)}>{t("auth.secondFactor.verify")}</Button>
        <Button type="button" variant="ghost" onClick={() => void signOut()}>{t("auth.secondFactor.other")}</Button>
      </form>
    </div>
  );
}
