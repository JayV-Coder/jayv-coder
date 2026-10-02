export type Provider = "github" | "gitlab" | "bitbucket";
export const PROVIDERS: Provider[] = ["github", "gitlab", "bitbucket"];

/** Desvincular só quando sobra outra forma de entrar: a senha ou outra
 * identidade (inclusive a `email`). */
export function canUnlink(providers: string[], hasPassword: boolean, provider: Provider) {
  if (!providers.includes(provider)) return false;
  return hasPassword || providers.some((other) => other !== provider);
}
