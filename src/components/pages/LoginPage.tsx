import { useState, type FormEvent } from "react";
import { signIn, signInWithGithub, signUp, useAuth } from "@/modules/auth";
import { reportError } from "@/modules/feedback";
import { useT } from "@/modules/i18n";
import { BrandMark } from "@/components/atoms";
import { FormField, LanguageSelect } from "@/components/molecules";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

/** A porta do app: e-mail e senha, ou GitHub pelo navegador. Os dados do
 * desenvolvedor moram no Supabase, então nada abre antes de alguém entrar. */
export function LoginPage() {
  const t = useT();
  const waiting = useAuth((state) => state.waitingBrowser);
  const [mode, setMode] = useState<"signIn" | "signUp">("signIn");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setNote("");
    try {
      if (mode === "signIn") await signIn(email.trim(), password);
      else if (await signUp(email.trim(), password)) { setNote(t("auth.checkEmail")); setMode("signIn"); }
    } catch (error) {
      reportError(error instanceof Error ? error.message : error);
    } finally {
      setBusy(false);
    }
  };

  const github = () => signInWithGithub().catch((error) => reportError(error instanceof Error ? error.message : error));

  return (
    <div className="grid min-h-screen place-items-center px-6">
      <div className="grid w-full max-w-sm gap-6">
        <div className="grid justify-items-center gap-3 text-center">
          <BrandMark />
          <h1 className="text-lg font-semibold">{t("auth.title")}</h1>
          <p className="text-sm text-muted-foreground">{t("auth.subtitle")}</p>
        </div>
        <form onSubmit={submit} className="grid gap-4">
          <FormField label={t("auth.email")} htmlFor="login-email">
            <Input id="login-email" type="email" autoComplete="email" required value={email} onChange={(event) => setEmail(event.target.value)} />
          </FormField>
          <FormField label={t("auth.password")} htmlFor="login-password">
            <Input id="login-password" type="password" autoComplete={mode === "signIn" ? "current-password" : "new-password"} required minLength={6} value={password} onChange={(event) => setPassword(event.target.value)} />
          </FormField>
          {note && <p role="status" className="text-[12px] text-muted-foreground">{note}</p>}
          <Button type="submit" disabled={busy}>{mode === "signIn" ? t("auth.signIn") : t("auth.signUp")}</Button>
          {/* Esperando o navegador, o botão segue ativo: se o retorno não vier
              (Redirect URL fora da lista no Supabase, aba fechada), dá para
              tentar de novo sem fechar o app. */}
          <Button type="button" variant="outline" disabled={busy} onClick={() => void github()}>
            {waiting ? t("auth.waitingBrowser") : t("auth.github")}
          </Button>
          <button type="button" className="text-xs text-muted-foreground hover:text-foreground" onClick={() => { setMode(mode === "signIn" ? "signUp" : "signIn"); setNote(""); }}>
            {mode === "signIn" ? t("auth.toSignUp") : t("auth.toSignIn")}
          </button>
        </form>
        <LanguageSelect />
      </div>
    </div>
  );
}
