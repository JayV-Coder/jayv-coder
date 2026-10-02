import { useState, type FormEvent } from "react";
import { PROVIDERS, passwordOk, requestPasswordReset, signIn, signInWithProvider, signUp, useAuth, type Provider } from "@/modules/auth";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { DISPLAY_NAME_MAX } from "@/modules/profile";
import { BrandMark, PROVIDER_NAMES } from "@/components/atoms";
import { FormField, LanguageSelect, PasswordRules, ProviderButton } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

type Mode = "signIn" | "signUp" | "reset";

/** A porta do app: e-mail e senha, ou GitHub, GitLab e Bitbucket pelo
 * navegador. Os dados do desenvolvedor moram no Supabase, então nada abre
 * antes de alguém entrar. */
export function LoginPage() {
  const t = useT();
  const waiting = useAuth((state) => state.waitingBrowser);
  const [mode, setMode] = useState<Mode>("signIn");
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [chosen, setChosen] = useState<Provider | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");

  const mismatch = mode === "signUp" && confirm.length > 0 && confirm !== password;
  const ready = mode !== "signUp" || (passwordOk(password) && confirm === password && name.trim().length > 0);

  const switchTo = (next: Mode) => {
    setMode(next);
    setNote("");
    setConfirm("");
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    setBusy(true);
    setNote("");
    try {
      if (mode === "signIn") await signIn(email.trim(), password);
      else if (mode === "reset") { await requestPasswordReset(email.trim()); switchTo("signIn"); setNote(t("auth.resetSent")); }
      else if (await signUp(email.trim(), password, name)) { switchTo("signIn"); setNote(t("auth.checkEmail")); }
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const viaProvider = (provider: Provider) => {
    setChosen(provider);
    signInWithProvider(provider).catch(reportError);
  };

  const title = mode === "reset" ? t("auth.reset.title") : t("auth.title");
  const subtitle = mode === "reset" ? t("auth.reset.description") : t("auth.subtitle");
  const action = mode === "signIn" ? t("auth.signIn") : mode === "signUp" ? t("auth.signUp") : t("auth.reset.send");

  return (
    <div className="grid min-h-screen place-items-center px-6 py-10">
      <div className="grid w-full max-w-sm gap-6">
        <div className="grid justify-items-center gap-3 text-center">
          <BrandMark />
          <h1 className="text-h3 font-semibold tracking-tight">{title}</h1>
          <p className="text-sm text-muted-foreground">{subtitle}</p>
        </div>
        <form onSubmit={submit} className="grid gap-4">
          <FormField label={t("auth.email")} htmlFor="login-email">
            <Input id="login-email" type="email" autoComplete="email" required value={email} onChange={(event) => setEmail(event.target.value)} />
          </FormField>
          {mode === "signUp" && (
            <FormField label={t("auth.displayName")} htmlFor="login-name">
              <Input id="login-name" autoComplete="nickname" required maxLength={DISPLAY_NAME_MAX} value={name} onChange={(event) => setName(event.target.value)} />
            </FormField>
          )}
          {mode !== "reset" && (
            <FormField label={t("auth.password")} htmlFor="login-password">
              <Input id="login-password" type="password" autoComplete={mode === "signIn" ? "current-password" : "new-password"} required value={password} onChange={(event) => setPassword(event.target.value)} />
            </FormField>
          )}
          {mode === "signUp" && (
            <>
              <PasswordRules password={password} />
              <FormField label={t("auth.confirmPassword")} htmlFor="login-confirm" error={mismatch ? t("auth.mismatch") : null}>
                <Input id="login-confirm" type="password" autoComplete="new-password" required aria-invalid={mismatch || undefined} value={confirm} onChange={(event) => setConfirm(event.target.value)} />
              </FormField>
            </>
          )}
          {mode === "signIn" && (
            <button type="button" className="justify-self-end text-xs text-muted-foreground hover:text-foreground" onClick={() => switchTo("reset")}>
              {t("auth.forgot")}
            </button>
          )}
          {note && <p role="status" className="text-xs text-muted-foreground">{note}</p>}
          <Button type="submit" disabled={busy || !ready}>{action}</Button>
        </form>
        {mode !== "reset" && (
          <div className="grid gap-2">
            <p className="flex items-center gap-3 text-caption uppercase tracking-wider text-muted-foreground before:h-px before:flex-1 before:bg-border after:h-px after:flex-1 after:bg-border">{t("auth.or")}</p>
            {/* Esperando o navegador, os botões seguem ativos: se o retorno não
                vier (Redirect URL fora da lista no Supabase, aba fechada), dá
                para tentar de novo sem fechar o app. */}
            {PROVIDERS.map((provider) => (
              <ProviderButton key={provider} provider={provider} disabled={busy} onClick={() => viaProvider(provider)}
                label={waiting && chosen === provider ? t("auth.waitingBrowser") : t("auth.continueWith", { provider: PROVIDER_NAMES[provider] })} />
            ))}
          </div>
        )}
        <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={() => switchTo(mode === "signIn" ? "signUp" : "signIn")}>
          {mode === "signIn" ? t("auth.toSignUp") : mode === "signUp" ? t("auth.toSignIn") : t("auth.backToSignIn")}
        </button>
        <LanguageSelect />
      </div>
    </div>
  );
}
