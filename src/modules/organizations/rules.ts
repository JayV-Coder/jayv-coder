export type Role = "owner" | "maintainer" | "member";
export const ROLES: Role[] = ["owner", "maintainer", "member"];
export const INVITE_ROLES: Exclude<Role, "owner">[] = ["maintainer", "member"];

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
