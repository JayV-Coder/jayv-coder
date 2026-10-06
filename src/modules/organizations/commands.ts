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
