# Organizações — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** organizações com papéis, convites e repositórios, e a associação de cada projeto local a uma organização pelo git remote.

**Architecture:** tabelas novas no Supabase com leitura por RLS e escrita só por RPCs `security definer` que conferem o papel; o núcleo Rust calcula as chaves dos remotes de cada projeto e as sincroniza em `projects.repo_keys`; o servidor cruza as chaves com os repositórios; o React fala com as RPCs pelo supabase-js.

**Tech Stack:** Postgres/pgTAP, Rust (rusqlite), React 19, zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-02-organizacoes-design.md`

## Global Constraints

- Identificadores em inglês; texto de tela só pelo i18n, com tradução nos 10 idiomas na migração.
- Escrita nas tabelas de organização só por RPC; RLS só de leitura.
- `profiles` continua legível só pelo dono.
- Chave de repositório: `host/caminho` em minúsculas, sem `.git`; hosts `github.com`, `gitlab.com`, `bitbucket.org`.
- Versão 0.16.0 → 0.17.0 nos cinco lugares.

## Review Focus

1. Último owner tentando sair, ser rebaixado ou removido → recusa `org.lastOwner` (Task 1).
2. URL de remote com porta SSH, usuário embutido ou barra final (`ssh://git@github.com:22/Acme/Api.git/`) → `github.com/acme/api` (Tasks 2 e 4).
3. Convite por e-mail aceito por conta com aquele e-mail ainda não confirmado → recusa (Task 1).
4. Projeto aberto antes de o app subir `repo_keys` (banco antigo, gatilho de update sem a coluna nova) → gatilho recriado e a coluna sobe (Task 3).
5. member chamando RPC de maintainer direto pela API → `org.forbidden` (Task 1).

---

### Task 1: Migração e pgTAP (`supabase`)
- [ ] Testes em `tests/organizations.test.sql` (papéis por RPC, último owner, convites, privacidade, `find_users`, `project_organization` com fork).
- [ ] `migrations/20261002120800_organizations.sql`: tabelas, RLS de leitura, trigger do último owner, `profiles.avatar_url`, `projects.repo_keys`, RPCs, `project_organization`, `my_project_organizations`.
- [ ] Conferir a sintaxe com o parser do Postgres (sem Docker aqui).

### Task 2: `repo_keys.rs` (Rust, TDD)
- [ ] Testes: formas de URL, hosts fora da lista, ordem `origin` primeiro, sem repetição, `.git` arquivo com `gitdir:` e `commondir`, pasta sem git.
- [ ] Implementação e `cargo test repo_keys`.

### Task 3: `projects.repo_keys` no SQLite e na sync
- [ ] Coluna nova (`ensure_project_repo_keys`), cálculo em `create_project` e `refresh_repo_keys` na abertura do banco.
- [ ] `outbox::TABLES` com `repo_keys`; `install` recria os gatilhos cujas colunas mudaram.
- [ ] Testes: a coluna nasce, o recálculo só grava quando muda, a fila recebe o update.

### Task 4: Front — regras puras e módulo `organizations`
- [ ] Vitest: `parseRepoUrl` (mesmos casos da Task 2), `slugify`, `slugOk`.
- [ ] `src/modules/organizations`: tipos, store, chamadas às RPCs, erros `org.*` traduzidos.

### Task 5: Telas
- [ ] View `organization`, item da lateral com contador, `OrganizationsPage`, `OrganizationPage` (Membros, Repositórios, Configurações), diálogo de nova organização, selo `@slug` no card do projeto.

### Task 6: Textos, verificação e entrega
- [ ] Chaves no `en.ts` e traduções na migração (10 idiomas, conferido por script).
- [ ] `npm run test:web`, `npm run typecheck`, `npm test` (cargo), build.
- [ ] Versão 0.17.0, commit e push no `main` dos dois repositórios.
