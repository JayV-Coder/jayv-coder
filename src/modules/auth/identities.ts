export type Provider = "github" | "gitlab" | "bitbucket";
export const PROVIDERS: Provider[] = ["github", "gitlab", "bitbucket"];

/** O nome de cada provedor é marca: não se traduz. */
export const PROVIDER_NAMES: Record<Provider, string> = { github: "GitHub", gitlab: "GitLab", bitbucket: "Bitbucket" };

/** Desvincular só quando sobra outra forma de entrar: a senha ou outra
 * identidade (inclusive a `email`). */
export function canUnlink(providers: string[], hasPassword: boolean, provider: Provider) {
  if (!providers.includes(provider)) return false;
  return hasPassword || providers.some((other) => other !== provider);
}

/** O que dizer quando o vínculo volta com `identity_already_exists`. O
 * Supabase dá esse código também quando a identidade já é desta conta (um
 * vínculo anterior que a tela não viu); só as identidades lidas de novo
 * separam os dois casos. */
export function linkOutcome(providers: string[], provider: Provider): "linked" | "taken" {
  return providers.includes(provider) ? "linked" : "taken";
}
