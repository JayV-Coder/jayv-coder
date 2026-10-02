# Organizações — design

Etapa B de 4. A (conta e perfil) está na 0.16.0; C (política de LLM por
organização e por projeto) e D (painel da organização com estatísticas e
portaria) usam o que esta etapa cria: os papéis e a associação de cada
projeto a uma organização.

## Objetivo

- Qualquer conta cria organizações e convida pessoas por `@usuário` ou por
  e-mail, com os papéis owner, maintainer e member.
- Owner ou maintainer cadastra os repositórios da organização (GitHub, GitLab,
  Bitbucket) pela URL.
- Um projeto local pertence a uma organização quando um remote do git da pasta
  casa com um repositório dela e o dono do projeto é membro.

## Papéis

| Ação | owner | maintainer | member |
|---|---|---|---|
| Ver organização, membros e repositórios | ✓ | ✓ | ✓ |
| Convidar e revogar convite | ✓ | ✓ | |
| Remover member | ✓ | ✓ | |
| Remover maintainer ou owner | ✓ | | |
| Trocar papel (inclusive promover a owner) | ✓ | | |
| Cadastrar e remover repositório | ✓ | ✓ | |
| Renomear | ✓ | ✓ | |
| Excluir a organização | ✓ | | |
| Sair | ✓ (não sendo o último owner) | ✓ | ✓ |

A organização nunca fica sem owner: um trigger recusa remover, rebaixar ou
tirar o último.

## Dados — repositório `supabase`

Migração `20261002120800_organizations.sql`.

- `organizations`: `id uuid`, `name` (1 a 80), `slug` único
  (`^[a-z0-9][a-z0-9-]{1,38}[a-z0-9]$`), `created_by`, `created_at`.
- `organization_members`: pk `(org_id, user_id)`, `role`
  (`owner|maintainer|member`), `joined_at`.
- `organization_invites`: `id`, `org_id`, `invited_user_id` **ou** `email`
  (minúsculas), `role` (`maintainer|member`), `invited_by`, `status`
  (`pending|accepted|declined|revoked`), `created_at`, `expires_at`
  (`created_at + 14 dias`). Índices únicos parciais: um pendente por
  `(org_id, invited_user_id)` e por `(org_id, email)`.
- `organization_repositories`: `id`, `org_id`, `provider`
  (`github|gitlab|bitbucket`), `path`, `repo_key` (`host/path` em minúsculas,
  único por organização), `added_by`, `created_at`.
- `profiles.avatar_url`: copiado do provedor (`avatar_url` ou `picture`) pelo
  trigger de cadastro e atualizado quando os metadados mudam.
- `projects.repo_keys text not null default '[]'`: lista JSON de chaves, com o
  `origin` primeiro; sobe pela fila de sync.

### Leitura (RLS)

- Membro lê a organização, os membros e os repositórios dela.
- Convite: os maintainers e owners da organização e o convidado (por
  `invited_user_id` ou pelo e-mail confirmado da conta).
- `profiles` continua só do dono. A lista de membros vem por
  `organization_members_view(org uuid)` (nome, `@usuário`, avatar, papel,
  entrada); a busca para convidar por `find_users(query text)` (prefixo do
  `@usuário`, mínimo 2 caracteres, até 8 resultados; só nome, `@usuário` e
  avatar). Não há busca por e-mail.

### Escrita (só por RPC, `security definer`)

`create_organization(name, slug)`, `rename_organization(org, name)`,
`delete_organization(org)`, `invite_member(org, username | email, role)`,
`revoke_invite(invite)`, `accept_invite(invite)`, `decline_invite(invite)`,
`set_member_role(org, user, role)`, `remove_member(org, user)`,
`leave_organization(org)`, `add_repository(org, provider, path)`,
`remove_repository(repository)`. Cada uma confere o papel pela tabela da
seção "Papéis" e falha com uma mensagem estável (`org.forbidden`,
`org.lastOwner`, `org.slugTaken`, `org.inviteExpired`, `org.alreadyMember`,
`org.userNotFound`, `org.repoTaken`, `org.emailUnconfirmed`) que a tela
traduz. `accept_invite` só aceita convite pendente e não vencido; o convite
por e-mail exige `email_confirmed_at` na conta.

### Associação de projeto

`project_organization(project text) returns uuid`: percorre `repo_keys` do
projeto na ordem e devolve a organização do primeiro que casa com um
repositório de uma organização da qual o dono do projeto é membro; empate
(mesma chave em duas organizações dele) fica com o cadastro mais antigo.
`my_project_organizations()` devolve `(project_id, org_id, slug, name)` dos
projetos do usuário.

## Núcleo — `jayv-coder/src-tauri`

- `repo_keys.rs`: lê `<pasta>/.git/config` (seguindo o arquivo `.git` com
  `gitdir:` e o `commondir` das worktrees), extrai `[remote "x"] url`,
  normaliza para `host/caminho` em minúsculas sem `.git` (formas
  `git@host:a/b.git`, `ssh://git@host[:porta]/a/b`, `https://[user@]host/a/b`),
  mantém só `github.com`, `gitlab.com` e `bitbucket.org`, ordena `origin`
  primeiro e o resto pelo nome do remote, sem repetir chave.
- `projects.repo_keys` no SQLite e em `outbox::TABLES`; calculado ao criar o
  projeto e recalculado ao abrir o banco (só grava se mudou). Os gatilhos da
  fila são recriados quando as colunas de uma tabela mudam.

## Telas — `jayv-coder/src`

- Lateral: item **Organizações** com o número de convites pendentes.
- `OrganizationsPage`: convites pendentes (Aceitar/Recusar) e os cards das
  minhas organizações (nome, `@slug`, meu papel, membros, repositórios);
  **Nova organização** (nome e slug sugerido).
- `OrganizationPage` (vista `organization`), abas:
  - **Membros**: lista; owner troca papel; quem pode remove; convidar com busca
    por `@usuário` ou e-mail e papel; convites pendentes com Revogar.
  - **Repositórios**: colar URL, ver provedor e caminho reconhecidos antes de
    salvar (`parseRepoUrl`, mesma regra do Rust); lista com Remover.
  - **Configurações**: renomear, sair, excluir (owner, digitando o slug).
  - member vê Membros e Repositórios só para leitura.
- Projetos: o card mostra `@slug` quando o projeto está associado.

## Fora do escopo

- Envio de e-mail para o convite (exige SMTP e Edge Function).
- Provar a posse do repositório pela API do provedor; GitHub/GitLab
  self-hosted.
- Política de LLM (C) e painel da organização (D).

## Testes

- pgTAP: cada RPC com cada papel; último owner; convite vencido, por e-mail
  sem confirmação, duplicado; privacidade (não membro não vê nada, membro não
  lê `profiles` de outro); `find_users`; `project_organization` com fork
  (`origin` pessoal, `upstream` da organização) e com não membro.
- Rust: normalização de URL, ordem dos remotes, worktree, pasta sem git.
- Vitest: `parseRepoUrl` (mesmos casos do Rust), `slugify` e `slugOk`.

## Entrega

Versão 0.16.0 → 0.17.0; commit e push no `main` do `supabase` e do
`jayv-coder`; o `supabase db push` é do dono do projeto.
