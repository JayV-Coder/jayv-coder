import { create } from "zustand";
import type { Session, User } from "@supabase/supabase-js";
import { getCurrent, onOpenUrl } from "@tauri-apps/plugin-deep-link";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, onCore } from "@/modules/core/bridge";
import { notify, reportError } from "@/modules/feedback";
import { t } from "@/modules/i18n";
import { readCallback } from "./callback";
import { receiveBillingLink } from "@/modules/plans";
import { CALLBACK_URL, supabase } from "./client";
import { authFailure } from "./errors";
import { canUnlink, linkOutcome, PROVIDER_NAMES, type Provider } from "./identities";
import { needsSecondFactor } from "./mfa";

export { authFailure } from "./errors";
export { canUnlink, PROVIDER_NAMES, PROVIDERS, type Provider } from "./identities";
export { CODE_MAX, codeDigits, codeOk } from "./code";
export { secretGroups, TOTP_LENGTH, totpDigits, totpOk } from "./mfa";
export { PASSWORD_MIN, PASSWORD_RULES, passwordOk, passwordRules, type PasswordRule } from "./password";

/** `secondFactor`: a senha (ou o provedor) passou, falta o código do app
 * autenticador. O núcleo ainda não recebeu o token. */
type Status = "loading" | "signedOut" | "secondFactor" | "signedIn";

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
  /** O app autenticador já confirmado, quando a conta tem um. */
  totpFactorId: string | null;
}

const SIGNED_OUT = { email: null, profile: null, recovering: false, providers: [], hasPassword: false, totpFactorId: null };

/** O app autenticador recém-cadastrado, até a pessoa digitar o primeiro
 * código: `uri` vira o QR code e `secret` é a chave para digitar à mão. */
export interface TotpEnrollment {
  factorId: string;
  uri: string;
  secret: string;
}

/** Enquanto a troca de senha entra de novo para conferir a atual, a sessão
 * passa um instante por `aal1`: o ouvinte não a repassa, e a troca entrega a
 * sessão final quando termina. */
let reauthenticating = false;

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
  // Com o app autenticador cadastrado, a senha sozinha não abre o app: o
  // token `aal1` não vai ao núcleo, e o banco também o recusa (migração
  // `second_factor` do repositório do Supabase).
  const { data: level } = await supabase.auth.mfa.getAuthenticatorAssuranceLevel();
  if (needsSecondFactor(level)) {
    await commands.clearSession().catch(reportError);
    useAuth.setState({ status: "secondFactor", email: session.user.email ?? null, profile: null, waitingBrowser: false });
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
  // A volta do pagamento no Stripe (`jayv://billing/...`) não é de login.
  if (receiveBillingLink(url)) return;
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
    if (reauthenticating && session) return;
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

/** As identidades, se há senha e se há app autenticador: a conta OAuth que
 * define senha não ganha a identidade `email`, então só o banco sabe
 * (`account_has_password`). */
export async function loadAccess() {
  const [identities, password, factors] = await Promise.all([supabase.auth.getUserIdentities(), supabase.rpc("account_has_password"), supabase.auth.mfa.listFactors()]);
  if (identities.error) fail(identities.error);
  if (factors.error) fail(factors.error);
  useAuth.setState({
    providers: (identities.data?.identities ?? []).map((identity) => identity.provider),
    hasPassword: password.data === true,
    // `totp` traz só os fatores já confirmados.
    totpFactorId: factors.data?.totp[0]?.id ?? null,
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
 * que o "secure password change" do Supabase pede. Com o app autenticador,
 * esse novo login volta a `aal1` e o Supabase só troca a senha em `aal2`: o
 * código do app (`code`) sobe a sessão de novo antes da troca. Se o código
 * falhar, a sessão fica em `aal1` e o app pede o segundo fator. */
export async function changePassword(current: string, next: string, code?: string) {
  const { email, totpFactorId } = useAuth.getState();
  if (!email) fail(new Error("account without email"));
  reauthenticating = true;
  try {
    const check = await supabase.auth.signInWithPassword({ email: email!, password: current });
    if (check.error) fail(check.error.code === "invalid_credentials" ? { code: "wrong_password" } : check.error);
    if (totpFactorId) {
      const verified = await supabase.auth.mfa.challengeAndVerify({ factorId: totpFactorId, code: code?.trim() ?? "" });
      if (verified.error) fail(verified.error);
    }
    const { error } = await supabase.auth.updateUser({ password: next });
    if (error) fail(error);
  } finally {
    reauthenticating = false;
    const { data } = await supabase.auth.getSession();
    await hand(data.session);
  }
}

/** O código do app autenticador depois da senha ou do provedor. O Supabase
 * avisa a sessão nova (`MFA_CHALLENGE_VERIFIED`) e o `hand` abre o app. */
export async function verifySecondFactor(code: string) {
  const { data, error } = await supabase.auth.mfa.listFactors();
  if (error) fail(error);
  const factor = data?.totp[0];
  if (!factor) fail({ code: "mfa_factor_not_found" });
  const verified = await supabase.auth.mfa.challengeAndVerify({ factorId: factor!.id, code: code.trim() });
  if (verified.error) fail(verified.error);
}

/** Cadastra o app autenticador. Um cadastro anterior que não chegou ao
 * primeiro código sai antes, para não acumular fatores pela metade. */
export async function enrollTotp(): Promise<TotpEnrollment> {
  const listed = await supabase.auth.mfa.listFactors();
  if (listed.error) fail(listed.error);
  for (const stale of listed.data?.all ?? []) {
    if (stale.factor_type !== "totp" || stale.status !== "unverified") continue;
    const removed = await supabase.auth.mfa.unenroll({ factorId: stale.id });
    if (removed.error) fail(removed.error);
  }
  const { data, error } = await supabase.auth.mfa.enroll({ factorType: "totp", issuer: "JayV" });
  if (error) fail(error);
  return { factorId: data!.id, uri: data!.totp.uri, secret: data!.totp.secret };
}

/** O primeiro código confirma o cadastro e já sobe a sessão para `aal2`. */
export async function confirmTotp(factorId: string, code: string) {
  const { error } = await supabase.auth.mfa.challengeAndVerify({ factorId, code: code.trim() });
  if (error) fail(error);
  await loadAccess();
}

export async function cancelTotp(factorId: string) {
  const { error } = await supabase.auth.mfa.unenroll({ factorId });
  if (error) fail(error);
}

/** Desligar pede um código atual do app: uma sessão esquecida aberta não
 * basta para tirar a proteção da conta. A sessão é renovada em seguida para
 * deixar de carregar o fator removido. */
export async function removeTotp(code: string) {
  const factorId = useAuth.getState().totpFactorId;
  if (!factorId) return;
  const verified = await supabase.auth.mfa.challengeAndVerify({ factorId, code: code.trim() });
  if (verified.error) fail(verified.error);
  const { error } = await supabase.auth.mfa.unenroll({ factorId });
  if (error) fail(error);
  await supabase.auth.refreshSession();
  await loadAccess();
}

/** O código que vai ao e-mail antes da primeira senha (de 6 a 10 dígitos,
 * conforme o projeto do Supabase; ver `code.ts`). */
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
