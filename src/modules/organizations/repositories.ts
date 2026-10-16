import type { RepositoryState } from "@/modules/core";
import type { StoredPolicy } from "./policy";
import type { Repository } from "./index";

/** De onde vem a política de LLM que o repositório tem: a dele mesmo, a da
 * organização, ou nenhuma. O chat da organização aplica a mais restrita de
 * todas juntas; esta é a regra que o repositório traz para a soma. */
export type PolicySource = "repository" | "organization" | "none";

/** Um repositório como o painel do chat da organização o mostra. `state` nulo
 * é o repositório da organização que não está dentro da pasta do chat;
 * `repository` nulo, o clone da pasta que a organização não cadastrou. */
export interface ChatRepository {
  key: string;
  state: RepositoryState | null;
  repository: Repository | null;
  policy: PolicySource;
}

function sourceOf(repository: Repository | null, policies: StoredPolicy[]): PolicySource {
  if (repository && policies.some((policy) => policy.repositoryId === repository.id)) return "repository";
  return policies.some((policy) => policy.repositoryId === null) ? "organization" : "none";
}

/** As chaves dos remotes de um clone: todas, quando o núcleo as manda (o
 * app antigo só mandava a primeira). */
const keysOf = (state: RepositoryState) => (state.keys?.length ? state.keys : state.key ? [state.key] : []);

/** Junta o que o git diz da pasta com o que a organização cadastrou: primeiro
 * os clones da pasta, na ordem do caminho; depois os repositórios da
 * organização que ficaram de fora. Um clone é do repositório cadastrado por
 * qualquer um dos remotes dele (`origin`, `upstream`…). */
export function chatRepositories(states: RepositoryState[], repositories: Repository[], policies: StoredPolicy[]): ChatRepository[] {
  const byKey = new Map(repositories.map((repository) => [repository.repoKey, repository]));
  const seen = new Set<string>();
  const inside = states.map((state) => {
    const keys = keysOf(state);
    const repository = keys.map((key) => byKey.get(key)).find((found) => found !== undefined) ?? null;
    for (const key of keys) seen.add(key);
    return { key: repository?.repoKey ?? state.key ?? (state.relative || state.path), state, repository, policy: sourceOf(repository, policies) };
  });
  const outside = repositories
    .filter((repository) => !seen.has(repository.repoKey))
    .map((repository) => ({ key: repository.repoKey, state: null, repository, policy: sourceOf(repository, policies) }));
  return [...inside, ...outside];
}

/** O nome curto do repositório na linha: o caminho dentro da pasta, ou a
 * chave, para a pasta que é ela mesma o repositório e o que está fora. */
export function repositoryLabel(item: ChatRepository): string {
  return item.state?.relative || item.repository?.path || item.key;
}

/** O comando que faz o git confiar numa pasta de outro dono (o git recusa
 * repositório de outro usuário: `dubious ownership`). Com `/`, que o git
 * aceita em todo sistema, e entre aspas, por causa dos espaços. */
export function safeDirectoryCommand(path: string): string {
  return `git config --global --add safe.directory "${path.replace(/\\/g, "/")}"`;
}
