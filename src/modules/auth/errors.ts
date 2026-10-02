import type { Text } from "@/modules/i18n";

/** Os códigos do Supabase Auth que a tela sabe explicar. */
const KNOWN: Record<string, string> = {
  identity_already_exists: "auth.identityTaken",
  weak_password: "auth.weakPassword",
  invalid_credentials: "auth.invalidCredentials",
  // Interno: a troca de senha recusa a senha atual com este código.
  wrong_password: "auth.wrongPassword",
  same_password: "auth.samePassword",
  reauthentication_not_valid: "auth.badCode",
  otp_expired: "auth.badCode",
  single_identity_not_deletable: "auth.lastIdentity",
  manual_linking_disabled: "auth.linkingDisabled",
  email_not_confirmed: "auth.emailNotConfirmed",
  mfa_verification_failed: "auth.mfa.badCode",
  mfa_challenge_expired: "auth.mfa.badCode",
  mfa_factor_not_found: "auth.mfa.missing",
  mfa_totp_enroll_not_enabled: "auth.mfa.disabled",
  mfa_totp_verify_not_enabled: "auth.mfa.disabled",
};

export function authFailure(error: unknown): unknown {
  const code = typeof error === "object" && error !== null ? (error as { code?: unknown }).code : undefined;
  if (typeof code === "string" && KNOWN[code]) return { key: KNOWN[code] } satisfies Text;
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && typeof (error as { message?: unknown }).message === "string") return (error as { message: string }).message;
  return error;
}
