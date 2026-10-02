import { create } from "zustand";
import type { Session, User } from "@supabase/supabase-js";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, onCore } from "@/modules/core/bridge";
import { notify, reportError } from "@/modules/feedback";
import { t } from "@/modules/i18n";
import { readCallback } from "./callback";
import { CALLBACK_URL, supabase } from "./client";
import { authFailure } from "./errors";
import { canUnlink, linkOutcome, PROVIDER_NAMES, type Provider } from "./identities";

export { authFailure } from "./errors";
export { canUnlink, PROVIDER_NAMES, PROVIDERS, type Provider } from "./identities";
export { PASSWORD_MIN, PASSWORD_RULES, passwordOk, passwordRules, type PasswordRule } from "./password";

type Status = "loading" | "signedOut" | "signedIn";

/** A sessão como a página de perfil a mostra. Nome e foto só existem quando o
 * login foi por um provedor; `provider` é o do último login (`email`,
 * `github`, `gitlab` ou `bitbucket`). */
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
  /** O provedor abriu no navegador e o app espera o link de volta. */
  waitingBrowser: boolean;
  /** O provedor que está sendo vinculado, até o link de volta chegar. */
  linking: Provider | null;
  /** A sessão veio do link de recuperação: falta escolher a senha nova. */
  recovering: boolean;
  /** As identidades da conta (`email`, `github`, `gitlab`, `bitbucket`). */
  providers: string[];
  hasPassword: boolean;
}

const SIGNED_OUT = { email: null, profile: null, recovering: false, providers: [], hasPassword: false };

export const useAuth = create<AuthState>(() => ({ status: "loading", waitingBrowser: false, linking: null, ...SIGNED_OUT }));

/** O erro já pronto para o `reportError`: a chave do i18n quando o Supabase
 * diz um código que a tela sabe explicar. */
const fail = (error: unknown): never => {
  throw authFailure(error);
};

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
    useAuth.setState({ status: "signedOut", linking: null, ...SIGNED_OUT });
    return;
  }
  try {
    const view = await commands.setSession(session.access_token);
    useAuth.setState({ status: "signedIn", email: view.email ?? session.user.email ?? null, profile: profileOf(session.user), waitingBrowser: false });
  } catch (error) {
    reportError(error);
    useAuth.setState({ status: "signedOut", ...SIGNED_OUT });
    return;
  }
  await loadAccess().catch(reportError);
}

/** O link de volta de um provedor (login ou vinculação), da confirmação de
 * e-mail ou da recuperação de senha: o supabase-js sabe qual pelo verificador
 * do PKCE que guardou. */
function receive(url: string) {
  const callback = readCallback(url);
  if (!callback) return;
  const { linking } = useAuth.getState();
  if ("failure" in callback) {
    useAuth.setState({ waitingBrowser: false, linking: null });
    if (linking && callback.reason === "identity_already_exists") void settleLink(linking);
    else reportError(callback.failure);
    return;
  }
  supabase.auth.exchangeCodeForSession(callback.code).then(({ error }) => {
    useAuth.setState({ waitingBrowser: false, linking: null });
    if (error) reportError(authFailure(error));
    else if (linking) notify(t("linked.done", { provider: PROVIDER_NAMES[linking] }));
  });
}

/** O vínculo voltou dizendo que a identidade já existe. Se ela já é desta
 * conta, o vínculo deu certo antes e só a tela não soube: relê e confirma.
 * Senão ela entra em outra conta do JayV, e o aviso diz qual provedor. */
async function settleLink(provider: Provider) {
  try {
    await loadAccess();
  } catch (error) {
    reportError(error);
    return;
  }
  const name = PROVIDER_NAMES[provider];
  if (linkOutcome(useAuth.getState().providers, provider) === "linked") notify(t("linked.done", { provider: name }));
  else reportError({ key: "linked.taken", params: { provider: name } });
}

export function connectAuth() {
  const { data } = supabase.auth.onAuthStateChange((event, session) => {
    // Fora da chamada do supabase-js: esperar dentro dela trava o cliente.
    setTimeout(() => {
      if (event === "PASSWORD_RECOVERY") useAuth.setState({ recovering: true });
      void hand(session);
    }, 0);
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
  if (error) fail(error);
}

/** Devolve `true` quando a conta precisa ser confirmada pelo e-mail. O nome
 * vai nos metadados e o banco o copia para o perfil. */
export async function signUp(email: string, password: string, displayName: string) {
  const { data, error } = await supabase.auth.signUp({ email, password, options: { emailRedirectTo: CALLBACK_URL, data: { display_name: displayName.trim() } } });
  if (error) fail(error);
  return !data.session;
}

export async function signInWithProvider(provider: Provider) {
  const { data, error } = await supabase.auth.signInWithOAuth({ provider, options: { redirectTo: CALLBACK_URL, skipBrowserRedirect: true } });
  if (error) fail(error);
  useAuth.setState({ waitingBrowser: true });
  await openUrl(data.url!);
}

export async function signOut() {
  const { error } = await supabase.auth.signOut();
  if (error) reportError(error);
}

/** As identidades e se há senha: a conta OAuth que define senha não ganha a
 * identidade `email`, então só o banco sabe (`account_has_password`). */
export async function loadAccess() {
  const [identities, password] = await Promise.all([supabase.auth.getUserIdentities(), supabase.rpc("account_has_password")]);
  if (identities.error) fail(identities.error);
  useAuth.setState({
    providers: (identities.data?.identities ?? []).map((identity) => identity.provider),
    hasPassword: password.data === true,
  });
}

export async function linkProvider(provider: Provider) {
  const { data, error } = await supabase.auth.linkIdentity({ provider, options: { redirectTo: CALLBACK_URL, skipBrowserRedirect: true } });
  if (error) fail(error);
  useAuth.setState({ waitingBrowser: true, linking: provider });
  await openUrl(data.url!);
}

export async function unlinkProvider(provider: Provider) {
  const { providers, hasPassword } = useAuth.getState();
  if (!canUnlink(providers, hasPassword, provider)) fail({ code: "single_identity_not_deletable" });
  const { data, error } = await supabase.auth.getUserIdentities();
  if (error) fail(error);
  const identity = data?.identities.find((known) => known.provider === provider);
  if (!identity) return;
  const unlinked = await supabase.auth.unlinkIdentity(identity);
  if (unlinked.error) fail(unlinked.error);
  await loadAccess();
}

export async function requestPasswordReset(email: string) {
  const { error } = await supabase.auth.resetPasswordForEmail(email, { redirectTo: CALLBACK_URL });
  if (error) fail(error);
}

/** Conferir a senha atual entrando de novo também renova a sessão, que é o
 * que o "secure password change" do Supabase pede. */
export async function changePassword(current: string, next: string) {
  const email = useAuth.getState().email;
  if (!email) fail(new Error("account without email"));
  const check = await supabase.auth.signInWithPassword({ email: email!, password: current });
  if (check.error) fail(check.error.code === "invalid_credentials" ? { code: "wrong_password" } : check.error);
  const { error } = await supabase.auth.updateUser({ password: next });
  if (error) fail(error);
}

/** O código de 6 dígitos que vai ao e-mail antes da primeira senha. */
export async function sendSetPasswordCode() {
  const { error } = await supabase.auth.reauthenticate();
  if (error) fail(error);
}

export async function setFirstPassword(code: string, password: string) {
  const { error } = await supabase.auth.updateUser({ password, nonce: code.trim() });
  if (error) fail(error);
  await loadAccess();
}

export async function finishRecovery(password: string) {
  const { error } = await supabase.auth.updateUser({ password });
  if (error) fail(error);
  useAuth.setState({ recovering: false });
  await loadAccess().catch(reportError);
}
