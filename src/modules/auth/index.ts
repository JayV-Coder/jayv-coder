import { create } from "zustand";
import type { Session } from "@supabase/supabase-js";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, onCore } from "@/modules/core/bridge";
import { reportError } from "@/modules/feedback";
import { CALLBACK_URL, supabase } from "./client";

type Status = "loading" | "signedOut" | "signedIn";

interface AuthState {
  status: Status;
  email: string | null;
  /** O GitHub abriu no navegador e o app espera o link de volta. */
  waitingBrowser: boolean;
}

export const useAuth = create<AuthState>(() => ({ status: "loading", email: null, waitingBrowser: false }));

/** O React guarda a sessão, e o Rust só a aceita depois de conferir a
 * assinatura: cada mudança passa por lá antes de a tela abrir. */
async function hand(session: Session | null) {
  if (!session) {
    await commands.clearSession().catch(reportError);
    useAuth.setState({ status: "signedOut", email: null });
    return;
  }
  try {
    const view = await commands.setSession(session.access_token);
    useAuth.setState({ status: "signedIn", email: view.email ?? session.user.email ?? null, waitingBrowser: false });
  } catch (error) {
    reportError(error);
    useAuth.setState({ status: "signedOut", email: null });
  }
}

/** O link de volta do GitHub ou da confirmação de e-mail. */
function receive(url: string) {
  let parsed: URL;
  try { parsed = new URL(url); } catch { return; }
  if (parsed.protocol !== "jayv:" || parsed.host !== "auth") return;
  const failure = parsed.searchParams.get("error_description");
  const code = parsed.searchParams.get("code");
  if (failure) {
    useAuth.setState({ waitingBrowser: false });
    reportError(failure);
  } else if (code) {
    supabase.auth.exchangeCodeForSession(code).catch((error) => {
      useAuth.setState({ waitingBrowser: false });
      reportError(error);
    });
  }
}

export function connectAuth() {
  const { data } = supabase.auth.onAuthStateChange((event, session) => {
    // Fora da chamada do supabase-js: esperar dentro dela trava o cliente.
    if (event !== "PASSWORD_RECOVERY") setTimeout(() => void hand(session), 0);
  });
  const offLink = onOpenUrl((urls) => urls.forEach(receive));
  // Aberto pelo próprio link, com o app fechado.
  void getCurrent().then((urls) => urls?.forEach(receive)).catch(() => {});
  const offExpired = onCore("link-changed", ({ link }) => {
    if (link === "expired") void supabase.auth.refreshSession();
  });
  return () => {
    data.subscription.unsubscribe();
    void offLink.then((unlisten) => unlisten());
    void offExpired.then((unlisten) => unlisten());
  };
}

export async function signIn(email: string, password: string) {
  const { error } = await supabase.auth.signInWithPassword({ email, password });
  if (error) throw error;
}

/** Devolve `true` quando a conta precisa ser confirmada pelo e-mail. */
export async function signUp(email: string, password: string) {
  const { data, error } = await supabase.auth.signUp({ email, password, options: { emailRedirectTo: CALLBACK_URL } });
  if (error) throw error;
  return !data.session;
}

export async function signInWithGithub() {
  const { data, error } = await supabase.auth.signInWithOAuth({ provider: "github", options: { redirectTo: CALLBACK_URL, skipBrowserRedirect: true } });
  if (error) throw error;
  useAuth.setState({ waitingBrowser: true });
  await openUrl(data.url);
}

export async function signOut() {
  const { error } = await supabase.auth.signOut();
  if (error) reportError(error);
}
