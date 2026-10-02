# Política de LLM — design

Etapa C de 4. A (conta e perfil) está na 0.16.0 e B (organizações) na 0.17.0.
Esta etapa usa o que B criou — os papéis e a associação de cada projeto a uma
organização pelo git remote — para que a organização diga com quais agentes e
modelos os projetos dela rodam e o que nunca sai da máquina. D (painel da
organização com estatísticas e portaria) vem depois.

## Objetivo

- Owner ou maintainer define a política de LLM da organização inteira e, se
  quiser, uma política própria para um repositório dela.
- Todo pedido de um chat cujo projeto pertence à organização roda sob a
  política efetiva: só os agentes e modelos permitidos entram no roteamento, os
  modos sem trava dos agentes são desligados e a privacidade e as regras da
  portaria de saída nunca ficam mais frouxas que as da organização.
- member vê a política, só para leitura.

## O que a política diz

| Campo | Valor | Efeito no app |
|---|---|---|
| `agents` | lista de `claude`, `codex`, `copilot`, `cursor`, ou nulo (todos) | agente fora da lista fica desligado |
| `blocked_models` | lista de `agente/modelo` (ex.: `claude/opus`) | modelo da lista fica desligado |
| `safe_agents` | booleano | desliga os modos sem trava (tabela abaixo) |
| `deny` | padrões glob | somados aos `deny` da privacidade de quem usa |
| `local_only` | padrões glob | somados aos `local_only` de quem usa |
| `redact_secrets` | booleano | `true` liga a remoção de segredos |
| `min_read`, `min_write`, `min_shell` | `allow`, `ask`, `deny` | a regra de saída fica no mais rígido entre a de quem usa e esta |

Os modos sem trava que `safe_agents` desliga:

| Agente | Opção | Vira |
|---|---|---|
| Claude Code | `permissionMode = bypassPermissions` | `default` |
| Codex | `sandbox = danger-full-access` | `workspace-write`, sem rede |
| Copilot | `toolAccess = all` | `edits` |
| Cursor | `force`, `approveMcps`, `sandbox = disabled` | `false`, `false`, `enabled` |

A política só aperta: nenhum campo liga o que quem usa desligou, nem afrouxa
uma regra. Ela não guarda chave nem comando de agente; cada pessoa continua
com os agentes da própria máquina.

### Organização e repositório

A política efetiva de um projeto junta a da organização e a do repositório
que associou o projeto (o primeiro remote que casou, a mesma regra de
`project_organization`), pela mais rígida:

- `agents`: interseção (nulo é "todos", então nulo com uma lista dá a lista);
- `blocked_models`, `deny`, `local_only`: união sem repetição;
- `safe_agents`, `redact_secrets`: basta um ligado;
- `min_*`: o mais rígido (`allow` < `ask` < `deny`).

Sem nenhuma das duas, o projeto não tem política.

## Dados — repositório `supabase`

Migração `20261002210000_llm_policies.sql`.

`organization_llm_policies`:

| Coluna | Tipo | Regra |
|---|---|---|
| `id` | `uuid` pk | |
| `org_id` | `uuid not null` | `references organizations on delete cascade` |
| `repository_id` | `uuid` | `references organization_repositories on delete cascade`; nulo é a da organização |
| `agents` | `text[]` | nulo ou de 1 a 4 itens da lista de agentes |
| `blocked_models` | `text[] not null default '{}'` | até 100, cada um `agente/modelo`, com os caracteres que o app aceita num id de modelo (`policy_models_ok`) |
| `deny`, `local_only` | `text[] not null default '{}'` | até 50 padrões de 1 a 200 caracteres |
| `safe_agents`, `redact_secrets` | `boolean not null default false` | |
| `min_read`, `min_write`, `min_shell` | `text not null default 'allow'` | `allow`, `ask`, `deny` |
| `updated_by` | `uuid` | `references auth.users on delete set null` |
| `updated_at` | `timestamptz not null default now()` | |

Índices únicos parciais: uma por organização (`repository_id is null`) e uma
por repositório.

- **RLS**: membro da organização lê. Escrita só por RPC.
- `set_llm_policy(org uuid, repository uuid, policy jsonb)`: owner ou
  maintainer; grava (ou troca) a política da organização (`repository` nulo)
  ou do repositório, que tem de ser da mesma organização. Os padrões chegam
  aparados e sem repetição. Falha com `org.forbidden`, `policy.invalid` (um
  `check` recusou) ou `policy.repository` (repositório de outra organização).
- `clear_llm_policy(org uuid, repository uuid)`: owner ou maintainer; volta a
  não ter política.
- `llm_policy_of(org uuid, repository uuid) returns jsonb`: a efetiva pela
  junção acima, ou nulo. Interna (sem `execute` para o cliente).
- `project_repository(project text) returns uuid`: o repositório que associa
  o projeto, pela regra de `project_organization`.
- `my_project_policies()` → `(project_id, org_id, org_slug, policy jsonb)`:
  os projetos de quem chama que têm política efetiva.

Chaves de erro novas, traduzidas na mesma migração: `policy.invalid`,
`policy.repository`. A migração também traduz as orientações do chat
(`guidance.*`), que até aqui não tinham tradução e apareciam como chave.

## Núcleo — `jayv-coder/src-tauri`

- `policy.rs`: `LlmPolicy` (o JSON da política efetiva), com
  `restrict_llm(&LlmSettings) -> LlmSettings` e
  `restrict_core(&CoreSettings) -> CoreSettings`, ambas puras.
  `restrict_llm` desliga agentes e modelos e, com `safe_agents`, troca as
  opções pela tabela acima (`AgentSettings::without_unsafe_modes` no `llm.rs`).
- Cache local `project_policies(project_id, org_slug, policy, fetched_at)` no
  SQLite do usuário, fora da fila de sync: é do servidor para cá, nunca o
  contrário.
- `Backend::project_policies()`: a sincronização chama
  `rpc/my_project_policies` no fim de cada volta e troca o cache inteiro.
  Falha nesta chamada só vai para o log: a volta não para, e o cache anterior
  continua valendo (inclusive sem rede).
- No atendimento (`desktop/queue.rs`), depois de focar a pasta do chat, o
  orquestrador recebe as configurações de quem usa passadas pela política do
  projeto do chat (`use_llm` e `use_core`). Chat de projeto sem política usa as
  configurações como estão.
- Quando a política deixa o pedido sem modelo, o chat responde
  `guidance.policyBlocked` (com `@slug`) em vez de `guidance.noFittingModel`.

## Telas — `jayv-coder/src`

- `OrganizationPage` ganha a aba **Política de LLM**
  (`OrganizationPolicy`):
  - escolha do escopo: a organização inteira ou um dos repositórios dela;
  - agentes permitidos (todos, ou uma seleção), modelos bloqueados
    (`agente/modelo`, um por linha), "Desligar os modos sem trava dos agentes",
    padrões de privacidade (`deny` e `local_only`, um por linha), "Sempre
    remover segredos" e o mínimo de cada regra de saída;
  - Salvar e "Remover política"; member vê o mesmo formulário desativado;
  - abaixo do escopo de repositório, uma nota de que vale junto com a da
    organização, pela mais rígida.
- `src/modules/organizations/policy.ts`: tipos, `emptyPolicy`, `policyOk`
  (as mesmas regras do `check`), `loadPolicies(org)`, `savePolicy`,
  `clearPolicy`.
- O card do projeto mostra, ao lado do `@slug`, um selo "Política de LLM"
  quando o projeto tem política efetiva.

## Fora do escopo

- Conferir a sintaxe dos padrões glob no servidor: um padrão inválido é
  ignorado pelo firewall, como já acontece com os de quem usa.

- Garantia contra quem altera o próprio app: a política vale no cliente
  oficial, que é quem roda os agentes. O servidor não vê o pedido.
- Teto de custo por modelo (a classe de custo é editada por quem usa) e
  orçamentos por organização: entram com o painel (D).
- Histórico de alterações da política.

## Testes

- pgTAP (`tests/llm_policies.test.sql`): cada papel em `set_llm_policy` e
  `clear_llm_policy`; `check` recusado vira `policy.invalid`; repositório de
  outra organização; não membro não lê; a junção organização + repositório
  (interseção de agentes, união de padrões, regra mais rígida);
  `my_project_policies` com fork e sem política.
- Rust: `restrict_llm` (agente fora, modelo bloqueado, modos sem trava dos
  quatro agentes, nada liga o que estava desligado), `restrict_core` (união,
  regra mais rígida), cache (troca inteira, chat → política), orientação
  `guidance.policyBlocked`.
- Vitest: `policyOk` e a conversão do formulário para o JSON da RPC.

## Entrega

Versão 0.18.0 → 0.19.0 (MINOR: tabela, RPCs, tela e comportamento novos e
compatíveis). PR no `supabase` (migração e testes) e no `jayv-coder` (núcleo,
tela, spec e plano). O `supabase db push` é do dono do projeto, antes de usar o
app novo; sem a migração, o app novo segue sem política.
