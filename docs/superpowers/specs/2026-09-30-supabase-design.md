# Supabase como fonte dos dados

Data: 2026-09-30 · Estado: aguardando revisão

## Problema

Tudo o que o JayV sabe mora na máquina onde ele roda: projetos, chats, turnos e
checks da portaria num SQLite (`workspace.rs`), as traduções em dez arquivos
TypeScript empacotados no build (`src/modules/i18n/messages/*.ts`), a
configuração dos LLMs em `llm_agents`/`llm_models` do mesmo SQLite, e as
perguntas e pesos do Jev escritos no Rust (`gatekeeper.rs`, `jev.rs`,
`asking.rs`). Mudar um texto ou uma instrução do Jev exige release, os dados não
acompanham o desenvolvedor entre máquinas, e a chave da TypeSafe precisa estar
no `.env` de cada instalação.

## Objetivo

O Supabase (projeto `exvsozyemolrjbjetqww`) passa a ser a fonte dos dados. Vários
usuários, cada um com os próprios dados; traduções e instruções do Jev globais,
editadas só por administradores. O SQLite continua, mas só como cache local e
fila de saída para o trabalho feito sem conexão.

## Decisões

| Tema | Decisão |
|---|---|
| Papel do Supabase | Fonte dos dados; o app lê e grava lá |
| Usuários | Vários, cada um com os próprios dados (RLS por `user_id`) |
| Conteúdo global | Traduções e instruções do Jev; escrita só por `admins` |
| Configuração dos LLMs | Por usuário, agentes e modelos |
| Offline | O app segue funcionando; escritas vão para a fila de saída, pedidos esperam a rede e são processados quando ela volta |
| Conflitos | Mensagens, turnos, eventos e checks só são acrescentados; o resto vale a edição mais recente (`updated_at`); exclusão por `deleted_at` |
| Login | E-mail e senha + GitHub (OAuth com PKCE), no React com `@supabase/supabase-js` |
| Quem valida | O Rust, pela assinatura ES256 do JWT contra o JWKS do projeto |
| Chave da TypeSafe | Só no servidor, como secret de uma Edge Function `jev` |
| Dados locais atuais | Descartados; nada é importado |
| Cliente Rust do Supabase | Escrito sobre o `reqwest` que o projeto já usa; o `supabase-lib-rs` fica de fora |

### Por que não o `supabase-lib-rs`

O crate (0.5.3, out/2025, um mantenedor) sempre põe a publishable key no
`Authorization` das Edge Functions (`functions.rs:276` e seguintes) e fixa o
cabeçalho do banco na construção do cliente. A função `jev` não saberia quem
chama, e o RLS por `auth.uid()` dependeria de recriar o cliente a cada
renovação de token. Sem auth (que foi para o React) e sem functions, sobraria
só o construtor de consultas — que para os nossos usos são um GET com filtros e
um POST de upsert.

## Arquitetura

```
React ──login (supabase-js, PKCE)──▶ Supabase Auth
  │  set_session(access_token) / clear_session
  ▼
Rust
  cloud/session.rs   valida o JWT (ES256, iss, aud, exp) e extrai o user_id
  cloud/remote.rs    PostgREST: select por synced_at, upsert por id
  cloud/jev.rs       POST /functions/v1/jev com o token do usuário
  local/cache.rs     SQLite por usuário, espelho das tabelas remotas
  local/outbox.rs    fila de saída, gravada na mesma transação da escrita
  sync.rs            sobe a fila, baixa as novidades, acorda o atendente
  desktop/queue.rs   o atendente de pedidos, agora só com rede e sessão válida
```

`cloud/` é o único módulo que conhece o Supabase. `workspace.rs` deixa de ser a
fonte e dá lugar a `local/`.

### Sessão

- O React mantém a sessão (o `supabase-js` renova sozinho) e a cada mudança
  chama o comando `set_session(access_token)`; no logout, `clear_session`.
- O Rust confere a assinatura contra
  `https://exvsozyemolrjbjetqww.supabase.co/auth/v1/.well-known/jwks.json`
  (hoje uma chave ES256), `iss`, `aud = authenticated` e `exp`. O JWKS fica no
  cache para validar sem rede.
- Nenhum comando que toca dados do usuário roda sem sessão validada.
- Sem rede e com token vencido, o último usuário validado continua valendo para
  o cache e a fila; a sincronização espera um token novo.
- GitHub: o React abre o navegador do sistema (`tauri-plugin-opener`), o retorno
  chega por `jayv://auth/callback?code=…` (`tauri-plugin-deep-link`) e o React
  chama `exchangeCodeForSession`.
- A URL do projeto e a publishable key vão embutidas no build. A secret key
  nunca entra no app nem no repositório.

## Tabelas no Supabase

As migrations ficam em `supabase/migrations/`, versionadas no repositório.

### Dados de cada usuário

`projects`, `chats`, `messages`, `turns`, `entry_checks`, `exit_checks`,
`turn_events`, `questions`, `llm_agents`, `llm_models` — as colunas atuais, com:

- `user_id uuid not null default auth.uid()` em todas, inclusive nas filhas,
  para o RLS ser `user_id = auth.uid()` sem join;
- ids UUID gerados na máquina; `messages.id` deixa de ser `AUTOINCREMENT`, e
  `llm_agents`/`llm_models` passam a ter chave `(user_id, id)` e
  `(user_id, agent, model)`;
- `updated_at` e `deleted_at` nas editáveis: `projects`, `chats`, `questions`,
  `llm_agents`, `llm_models`;
- `synced_at timestamptz not null default now()`, reescrito por trigger em todo
  insert e update — o cursor do download usa o relógio do servidor;
- trigger de edição mais recente nas editáveis: um update com `updated_at`
  anterior ao gravado é descartado;
- `chats.code` único por `(user_id, code)`; `turns` único por `(chat_id, ordinal)`;
- `projects.root_path` sai: a pasta é da máquina e mora em `local_paths` no
  SQLite.

### Conteúdo global

| Tabela | Colunas |
|---|---|
| `locales` | `id`, `name`, `rtl`, `position` |
| `translations` | `locale`, `key`, `value jsonb` (texto ou formas de plural) |
| `jev_questions` | `set` (`entry`, `routing`, `verification`, `asking`), `id`, `body jsonb` no formato que o Rust monta hoje, `position` |
| `jev_parameters` | `key`, `value jsonb` — pesos, `SCOPE_DEMAND`, `BLOCK_MARGIN`, `SCOPE_LEVELS`, `NOUL_LINE` |
| `admins` | `user_id` |
| `jev_usage` | `user_id`, `day`, `calls` — limite diário da função `jev` |

### RLS

- Tabelas do usuário: `select`, `insert`, `update` com `user_id = auth.uid()`;
  sem `delete` (exclusão é `deleted_at`).
- `locales` e `translations`: leitura para `anon` e `authenticated` (a tela de
  login precisa delas).
- `jev_questions` e `jev_parameters`: leitura para `authenticated`.
- Escrita no conteúdo global: só quem está em `admins`.
- `jev_usage`: só a Edge Function escreve (service role).

### Edge Function `jev`

Recebe `{ set, state }`. Valida o JWT, confere e soma `jev_usage`, carrega as
perguntas do `set` em `jev_questions`, chama a TypeSafe com o secret
`TYPESAFE_API_KEY` e devolve a avaliação sem mexer nela. O app não manda
perguntas: a chave não vira API de uso livre.

## Sincronização

### SQLite local

Um arquivo por usuário, `workspace-<user_id>.sqlite3` na pasta de dados do app.
Espelha as tabelas remotas e acrescenta:

- `outbox(seq, table, row_id, payload, status, attempts, error)`;
- `sync_state(table, cursor)` — o último `synced_at` visto;
- `local_paths(project_id, root_path)`;
- `jwks` e o último `user_id` validado.

### Escrita

Todo comando grava no cache e na `outbox` na mesma transação, emite o evento
para a tela e toca o sino da sincronização. A tela nunca espera a rede.

### Upload

A `outbox` sai em ordem de `seq`, como upsert por id
(`Prefer: resolution=merge-duplicates`). Reenviar é idempotente. Exclusão é um
upsert com `deleted_at`.

### Download

Por tabela, `synced_at > cursor` ordenado por `synced_at`, em páginas de 1000,
aplicado no cache com a mesma regra de edição mais recente.

### Quando roda

No login, na volta da rede, depois de escritas (agrupadas), e a cada 60 s. Sem
Realtime por enquanto.

### Erros

| Situação | Tratamento |
|---|---|
| Sem rede, 5xx | Fica na fila; backoff de 5 s a 60 s; a tela mostra offline |
| 401 | Emite `session-expired`; o React renova e chama `set_session`; a fila retoma |
| 409 em `(chat_id, ordinal)` | Renumera o turno para o próximo livre e reenvia |
| Outro 4xx | Item marcado como `failed` com o motivo; a fila segue; a tela mostra "N alterações não sincronizadas" |

### Atendente de pedidos

`queue.rs` só pega pedido com rede e sessão válida — o Jev agora é a função
`jev`. Offline os pedidos ficam `queued`; o sino de volta da rede acorda o
atendente, que processa em ordem.

## Instruções do Jev

`entry_questions`, `routing_questions`, `verification_questions` e as perguntas
de `asking.rs` saem do Rust e viram o seed de `jev_questions`. As constantes
numéricas viram o seed de `jev_parameters`. O Rust carrega os parâmetros do
cache e os valida ao carregar; se faltar algo obrigatório, cai nas heurísticas
locais que já existem (`heuristic_entry` e companhia) em vez de quebrar.

As regras da portaria de saída continuam no `config.yaml`, fora deste trabalho.

## i18n

- `en.ts` fica no repositório como fonte das chaves (`Key`) e fallback embutido.
- Os outros nove idiomas saem do build; um script os converte no seed SQL de
  `translations` e depois eles são removidos.
- A tela pede as traduções ao Rust, que responde do cache e atualiza pelo
  Supabase. O seletor de idiomas lê `locales`.

## Fora do escopo

- Tela de administração no app: conteúdo global se edita pelo painel do Supabase.
- Realtime.
- Importar o SQLite atual.
- Projetos compartilhados entre usuários.

## Testes

- **Rust:** `sync.rs` contra um remoto falso — ordem da fila, edição mais
  recente, renumeração no 409, offline, 401 sem rede. `session.rs` com JWTs
  assinados por uma chave ES256 de teste: válido, vencido, emissor errado,
  assinatura errada.
- **Banco:** pgTAP no Supabase local (`supabase test db`) — um usuário não lê
  nem grava dados de outro; só `admins` escreve no conteúdo global; `anon` lê
  traduções e não lê `jev_questions`.
- **Seed:** todas as chaves de `en.ts` em todos os idiomas; todas as perguntas
  obrigatórias de cada conjunto do Jev presentes.

## Segurança

A secret key do projeto foi colada numa conversa e deve ser rotacionada no
painel antes de ir a produção. Migrations e secrets da função se aplicam com
`supabase login` + `supabase link` + `supabase secrets set`, pela CLI do
desenvolvedor.
