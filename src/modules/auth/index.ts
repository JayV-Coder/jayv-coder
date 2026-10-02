import { create } from "zustand";
import type { Session, User } from "@supabase/supabase-js";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, onCore } from "@/modules/core/bridge";
import { reportError } from "@/modules/feedback";
import { CALLBACK_URL, supabase } from "./client";

type Status = "loading" | "signedOut" | "signedIn";

/** A conta como a página de perfil a mostra. Nome e foto só existem quando o
 * login foi pelo GitHub; `provider` é `email` ou `github`. */
export interface Profile {
  name: string | null;
  avatarUrl: string | null;
  provider: string;
  createdAt: string | null;
  lastSignInAt: string | null;
}

interface AuthState {
  status: Status;
  email: string | null;
  profile: Profile | null;
  /** O GitHub abriu no navegador e o app espera o link de volta. */
  waitingBrowser: boolean;
}

export const useAuth = create<AuthState>(() => ({ status: "loading", email: null, profile: null, waitingBrowser: false }));

const text = (value: unknown) => (typeof value === "string" && value.trim() ? value.trim() : null);

function profileOf(user: User): Profile {
  const meta = user.user_metadata ?? {};
  return {
    name: text(meta.full_name) ?? text(meta.name) ?? text(meta.user_name) ?? text(meta.preferred_username),
    avatarUrl: text(meta.avatar_url) ?? text(meta.picture),
    provider: text(user.app_metadata?.provider) ?? "email",
    createdAt: user.created_at ?? null,
    lastSignInAt: user.last_sign_in_at ?? null,
  };
}

/** O React guarda a sessão, e o Rust só a aceita depois de conferir a
 * assinatura: cada mudança passa por lá antes de a tela abrir. */
async function hand(session: Session | null) {
  if (!session) {
    await commands.clearSession().catch(reportError);
    useAuth.setState({ status: "signedOut", email: null, profile: null });
    return;
  }
  try {
    const view = await commands.setSession(session.access_token);
    useAuth.setState({ status: "signedIn", email: view.email ?? session.user.email ?? null, profile: profileOf(session.user), waitingBrowser: false });
  } catch (error) {
    reportError(error);
    useAuth.setState({ status: "signedOut", email: null, profile: null });
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
