/** As permissões de comandos da organização: o que os agentes nunca rodam nos
 * projetos dela. Vêm do servidor junto da política do projeto
 * (`my_project_policies`, `blocked_commands` e `command_sources`) e são
 * cadastradas no site. Cada regra é o programa e até dois subcomandos (`git`,
 * `git push`); o programa sozinho vale para todos os subcomandos. */

/** O catálogo do seletor de permissões do chat: o mesmo do site, com os
 * programas e os subcomandos que mais pesam. */
export const COMMAND_CATALOG: { tool: string; subcommands: string[] }[] = [
  { tool: "git", subcommands: ["status", "diff", "log", "show", "branch", "checkout", "switch", "add", "commit", "restore", "reset", "stash", "merge", "rebase", "cherry-pick", "tag", "fetch", "pull", "push", "clone", "remote", "clean"] },
  { tool: "npm", subcommands: ["install", "ci", "run", "test", "publish", "uninstall", "update", "exec"] },
  { tool: "pnpm", subcommands: ["install", "add", "remove", "run", "test", "publish", "update", "dlx"] },
  { tool: "yarn", subcommands: ["install", "add", "remove", "run", "test", "publish", "upgrade"] },
  { tool: "pip", subcommands: ["install", "uninstall", "download"] },
  { tool: "cargo", subcommands: ["build", "test", "run", "install", "publish", "add", "update"] },
  { tool: "go", subcommands: ["build", "test", "run", "get", "install", "mod"] },
  { tool: "docker", subcommands: ["run", "build", "compose", "push", "pull", "rm", "exec"] },
  { tool: "kubectl", subcommands: ["get", "apply", "delete", "exec"] },
  { tool: "gh", subcommands: ["pr", "issue", "release", "repo", "api"] },
  { tool: "terraform", subcommands: ["plan", "apply", "destroy"] },
  { tool: "make", subcommands: [] },
  { tool: "curl", subcommands: [] },
  { tool: "wget", subcommands: [] },
  { tool: "ssh", subcommands: [] },
  { tool: "sudo", subcommands: [] },
  { tool: "rm", subcommands: [] },
];

/** As regras bloqueadas de um projeto, cada uma com as organizações que a bloqueiam. */
export type BlockedCommands = Record<string, string[]>;

const words = (text: string) => text.trim().split(/\s+/).filter(Boolean);
const startsWith = (long: string[], short: string[]) => short.length > 0 && short.length <= long.length && short.every((word, index) => word === long[index]);

/** As organizações que bloqueiam o comando: as das regras que são o começo dele. */
export function blockersOf(blocked: BlockedCommands, command: string): string[] {
  const mine = words(command);
  const orgs: string[] = [];
  for (const [rule, found] of Object.entries(blocked)) {
    if (startsWith(mine, words(rule))) for (const org of found) if (!orgs.includes(org)) orgs.push(org);
  }
  return orgs;
}

/** O mesmo, para liberar `rule` inteira: vale se a regra bloqueada é o começo
 * dela ou se ela é o começo da bloqueada (liberar `git` liberaria `git push`). */
export function conflictsWith(blocked: BlockedCommands, rule: string): string[] {
  const mine = words(rule);
  const orgs: string[] = [];
  for (const [blockedRule, found] of Object.entries(blocked)) {
    const theirs = words(blockedRule);
    if (startsWith(mine, theirs) || startsWith(theirs, mine)) for (const org of found) if (!orgs.includes(org)) orgs.push(org);
  }
  return orgs;
}

/** As regras bloqueadas do projeto a partir da política que o servidor mandou:
 * `command_sources` diz quem bloqueia cada uma; sem ele, vale a organização do projeto. */
export function blockedCommandsOf(policy: unknown, orgSlug: string): BlockedCommands {
  const found: BlockedCommands = {};
  const row = (policy ?? {}) as { blocked_commands?: unknown; command_sources?: unknown };
  const sources = row.command_sources && typeof row.command_sources === "object" ? row.command_sources as Record<string, unknown> : {};
  const listed = Array.isArray(row.blocked_commands) ? row.blocked_commands.map(String) : Object.keys(sources);
  const slugs = orgSlug.split(", ").filter(Boolean);
  for (const rule of listed) {
    const from = Array.isArray(sources[rule]) ? (sources[rule] as unknown[]).map(String) : slugs;
    found[rule] = from.length > 0 ? from : slugs;
  }
  return found;
}

/** As permissões do chat (`shell`, `git`, `network`) que as organizações
 * desligam em um projeto, cada uma com as organizações que a desligam. Vêm
 * do servidor em `blocked_grants` e `grant_sources`, cadastradas na aba Chat
 * do site. */
export type BlockedGrants = Record<string, string[]>;

export const GRANT_KINDS = ["shell", "git", "network"] as const;
export type GrantKind = (typeof GRANT_KINDS)[number];

export function blockedGrantsOf(policy: unknown, orgSlug: string): BlockedGrants {
  const found: BlockedGrants = {};
  const row = (policy ?? {}) as { blocked_grants?: unknown; grant_sources?: unknown };
  const sources = row.grant_sources && typeof row.grant_sources === "object" ? row.grant_sources as Record<string, unknown> : {};
  const listed = Array.isArray(row.blocked_grants) ? row.blocked_grants.map(String) : Object.keys(sources);
  const slugs = orgSlug.split(", ").filter(Boolean);
  for (const kind of listed) {
    if (!(GRANT_KINDS as readonly string[]).includes(kind)) continue;
    const from = Array.isArray(sources[kind]) ? (sources[kind] as unknown[]).map(String) : slugs;
    found[kind] = from.length > 0 ? from : slugs;
  }
  return found;
}

const NETWORK_WORDS = ["install", "i", "add", "ci", "fetch", "pull", "push", "clone", "update", "upgrade", "download", "sync"];
const GIT_REMOTE_WORDS = ["fetch", "pull", "push", "clone", "ls-remote", "submodule"];

/** As organizações cuja permissão do chat desligada cobre a regra do
 * catálogo: "rodar comandos" cobre todas, "Git" as do git e "rede" as que
 * acessam a rede (instalar, baixar, falar com o remoto). O mesmo recorte do
 * núcleo (`blocking_kinds`). */
export function grantBlockersOf(blocked: BlockedGrants, rule: string): string[] {
  const [program = "", sub = ""] = words(rule);
  const network = program === "git"
    ? GIT_REMOTE_WORDS.includes(sub)
    : ["curl", "wget", "gh"].includes(program) || words(rule).slice(1, 3).some((word) => NETWORK_WORDS.includes(word));
  const orgs: string[] = [];
  for (const [kind, found] of Object.entries(blocked)) {
    const covers = kind === "shell" || (kind === "git" && program === "git") || (kind === "network" && network);
    if (covers) for (const org of found) if (!orgs.includes(org)) orgs.push(org);
  }
  return orgs;
}
