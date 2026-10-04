import type { Provider } from "@/modules/auth";

const HOSTS: Record<string, Provider> = { "github.com": "github", "gitlab.com": "gitlab", "bitbucket.org": "bitbucket" };

export type Role = "owner" | "maintainer" | "member";
export const ROLES: Role[] = ["owner", "maintainer", "member"];
export const INVITE_ROLES: Exclude<Role, "owner">[] = ["maintainer", "member"];

export interface RepoRef { provider: Provider; path: string; key: string }

/** A mesma normalização do núcleo (`src-tauri/crates/jayv-orgs/src/repo_keys.rs`): `host/caminho`
 * em minúsculas, sem `.git`, só dos três provedores. Aceita também a URL sem
 * esquema (`github.com/acme/api`), como se cola de uma barra de endereço. */
export function parseRepoUrl(url: string): RepoRef | null {
  const text = url.trim();
  let host: string | undefined;
  let path: string | undefined;
  const scheme = text.indexOf("://");
  if (scheme >= 0) {
    const rest = text.slice(scheme + 3);
    const slash = rest.indexOf("/");
    if (slash < 0) return null;
    host = rest.slice(0, slash).split("@").pop()?.split(":")[0];
    path = rest.slice(slash + 1);
  } else if (/^[^/]*:/.test(text)) {
    const colon = text.indexOf(":");
    host = text.slice(0, colon).split("@").pop();
    path = text.slice(colon + 1);
  } else {
    const slash = text.indexOf("/");
    if (slash < 0) return null;
    host = text.slice(0, slash);
    path = text.slice(slash + 1);
  }
  const provider = host ? HOSTS[host.toLowerCase()] : undefined;
  if (!provider || path === undefined) return null;
  let clean = path.replace(/^\/+|\/+$/g, "").toLowerCase();
  if (clean.endsWith(".git")) clean = clean.slice(0, -4).replace(/\/+$/, "");
  const parts = clean.split("/").filter(Boolean);
  if (parts.length < 2 || parts.some((part) => !/^[a-z0-9._-]+$/.test(part))) return null;
  const joined = parts.join("/");
  return { provider, path: joined, key: `${host!.toLowerCase()}/${joined}` };
}

export const SLUG_MAX = 40;

/** A mesma regra de `organizations.slug`: 3 a 40 caracteres, minúsculas,
 * dígitos e `-`, começando e terminando em letra ou dígito. */
export const slugOk = (slug: string) => /^[a-z0-9][a-z0-9-]{1,38}[a-z0-9]$/.test(slug);

/** O slug sugerido a partir do nome: sem acento, o resto vira `-`. */
export function slugify(name: string) {
  return name.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase()
    .replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "").slice(0, SLUG_MAX).replace(/-+$/, "");
}

/** O que cada papel pode fazer, igual às RPCs (o banco confere de novo). */
export const can = {
  manage: (role: Role | undefined) => role === "owner" || role === "maintainer",
  changeRoles: (role: Role | undefined) => role === "owner",
  remove: (mine: Role | undefined, theirs: Role) => mine === "owner" || (mine === "maintainer" && theirs === "member"),
  delete: (role: Role | undefined) => role === "owner",
};
