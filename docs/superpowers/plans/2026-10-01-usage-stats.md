# Estatísticas de uso — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans
> para implementar tarefa por tarefa. Os passos usam checkbox (`- [ ]`).

**Goal:** registrar cada gasto de tokens, cada leitura do limite do plano e
cada ação do Jev, e mostrá-los por chat, por projeto e no total.

**Architecture:** um módulo `usage` no núcleo recebe os registros por uma
pia global (`usage::sink`). O escopo (projeto, chat, turno) vem de um
`tokio::task_local!` aberto pela fila de turnos, então nenhum provedor precisa
saber de chats. Um gravador no desktop grava os registros em três tabelas
espelhadas pelo outbox. A tela consulta `usage_report(scope, period)` e
escuta `usage-recorded` e `quota-changed`.

**Tech Stack:** Rust (tokio, rusqlite), Tauri 2, React 19 + zustand,
`recharts`, Supabase (Postgres + edge function Deno).

**Spec:** `docs/superpowers/specs/2026-10-01-usage-stats-design.md`

## Global Constraints

- Identificadores em inglês; comentários podem ser em português.
- Nenhum texto para pessoa fixo no código: `Text` no núcleo, `useT()` na
  tela; chave nova em `en.ts` + migração com os 10 idiomas.
- Dado gravado (`source`, `kind`, `precision`, `window`) é identificador em
  inglês.
- Nada de nome, caminho ou regra de projeto do usuário em código, teste ou
  fixture.
- Versão 0.10.0 nos cinco lugares.

## Review Focus

- CLI que muda o formato da saída (Claude `/usage`, arquivo do Copilot): cai
  para `estimated`/"indisponível", nunca derruba o turno — testes de parser
  com entrada inesperada (Tarefas 3, 5).
- Chat apagado: o uso continua no global e aparece como "(chat removido)" —
  teste em `usage::query` (Tarefa 2).
- Duas máquinas: linhas só acrescentadas, sem conflito no sync — teste do
  outbox (Tarefa 2).
- Período vazio: a tela mostra zeros e "—", sem quebrar (Tarefa 9).
- CLI sem login/não instalado na leitura do limite: agente pulado com motivo
  (Tarefa 6).

---

### Task 1: módulo `usage` — tipos, pia e escopo

**Files:** Create `src-tauri/src/usage/mod.rs`; Modify `src-tauri/src/lib.rs`.

**Produces:**
- `pub enum Precision { Reported, Estimated, Legacy }` (`as_str`).
- `pub struct Spend { source, model, input_tokens:u64, output_tokens:u64,
  cache_read_tokens:u64, cache_write_tokens:u64, cost_usd:Option<f64>,
  duration_ms:u64, success:bool, precision:Precision }`.
- `pub struct JevMark { kind:String, amount:f64, precision:Precision }`.
- `pub struct Quota { agent, window, used_percent:Option<f64>,
  resets_at:Option<String>, plan:Option<String> }`.
- `pub struct Scope { project_id, chat_id, turn_id: Option<String> }`;
  `pub async fn within<F>(scope:Scope, work:F)->F::Output` (task-local).
- `pub enum Entry { Spend(Scope,Spend), Jev(Scope,JevMark), Quota(Quota) }`.
- `pub fn install(sender)`, `pub fn spend(Spend)`, `pub fn mark(JevMark)`,
  `pub fn quota(Quota)` — no-op sem pia instalada (CLI e testes).

- [ ] Teste: `spend` dentro de `within` chega à pia com o escopo; fora, com
  escopo vazio; sem pia, não entra em pânico.
- [ ] Implementar e rodar `cargo test usage::`.

### Task 2: tabelas, gravação, consulta e sync

**Files:** Create `src-tauri/src/usage/store.rs`; Modify
`src-tauri/src/workspace.rs` (`prepare` chama `usage::store::ensure`),
`src-tauri/src/local/outbox.rs` (`TABLES` 10→13).

**Produces:**
- `store::ensure(&Connection)` cria `usage_records`, `quota_snapshots`,
  `jev_records` + índices, e copia `turn_events` `done` como `legacy` uma vez
  (marca em `app_metadata`).
- `store::write(&Connection,&Entry,machine:&str)->Result<bool>` (quota igual
  em menos de 1 min é descartada → `false`).
- `store::report(&Connection,&Query)->Result<Report>` com
  `Query{scope:ReportScope, from:Option<String>, to:Option<String>,
  utc_offset_minutes:i32}`, `ReportScope = Global | Project(id) | Chat(id)`.
- `Report{totals, daily:Vec<Day>, by_source, by_model, by_project, by_chat,
  quotas:Vec<QuotaView>, jev:JevSummary, estimated_share:f64}`.
- `WorkspaceStore::usage_write`, `WorkspaceStore::usage_report`.

- [ ] Testes: escopos e períodos; precisões misturadas → `estimated_share`;
  chat apagado continua no global e por chat com `title=None`; migração
  `legacy`; as três tabelas geram entrada no outbox; quota repetida descartada.
- [ ] Implementar; `cargo test usage::store`.

### Task 3: Claude — uso do `result` e sinal de limite

**Files:** Create `src-tauri/src/usage/claude.rs`; Modify
`src-tauri/src/providers.rs` (laço do `CliProvider::chat_stream`).

**Produces:** `claude::spends(&Value)->Vec<Spend>` (um por modelo de
`modelUsage`; sem ele, um do `usage`), `claude::is_rate_limit(&Value)->bool`,
`claude::parse_usage_text(&str)->Option<Vec<Quota>>`.

`CliProvider` ganha um `Meter` (por agente: `claude|codex|copilot|other`) que
lê cada linha antes do `classify`; no fim, se o meter não colheu nada, emite
`Spend` `estimated` (`prompt/4`, saída `response/4`). Um `rate_limit_event`
pede `usage::refresh_quota("claude")` (Tarefa 6).

- [ ] Testes com fixtures (result com `modelUsage`, sem `modelUsage`, com
  `is_error`; `/usage` normal e texto desconhecido).
- [ ] Implementar; `cargo test usage::claude providers::`.

### Task 4: Codex — `--json`

**Files:** Create `src-tauri/src/usage/codex.rs`; Modify `src-tauri/src/llm.rs`
(`CodexOptions::args` ganha `--json`), `src-tauri/src/providers.rs`
(`said` lê `item.completed` com `agent_message`; `bookkeeping` cobre os
eventos de controle do Codex).

**Produces:** `codex::spend(&Value,model)->Option<Spend>`
(`turn.completed`), `codex::quotas(&Value)->Vec<Quota>` (qualquer evento com
`rate_limits`), `codex::latest_session_quotas(dir)->Vec<Quota>`.

- [ ] Testes com fixtures (`turn.completed`, `token_count` com
  `rate_limits`, `primary:null`, mensagem do agente vira texto do chat).
- [ ] Implementar; `cargo test usage::codex providers::`.

### Task 5: Copilot — arquivo de uso

**Files:** Create `src-tauri/src/usage/copilot.rs`; Modify `src-tauri/src/llm.rs`
(sem `--silent` na saída de stats: entra `--usage-output-file {usage_file}`),
`src-tauri/src/providers.rs` (troca `{usage_file}` por um arquivo temporário e
lê no fim).

**Produces:** `copilot::spends(&Value,model)->Vec<Spend>` tolerante a campos
ausentes (`premium_requests`, tokens por modelo).

- [ ] Testes: arquivo completo, parcial, ilegível → `estimated`.
- [ ] Implementar; `cargo test usage::copilot`.

### Task 6: leitura do limite do plano

**Files:** Create `src-tauri/src/usage/quota.rs`.

**Produces:** `quota::read(agent:&str, command:&str)->QuotaReading`
(`Ok(Vec<Quota>)` | `Unavailable(Text)`), `quota::refresh_quota(agent)`
(com intervalo mínimo de 2 min por agente) e o comando
`refresh_quotas` no desktop.

- [ ] Testes do intervalo mínimo e do caminho "não instalado".
- [ ] Implementar.

### Task 7: Jev — custo, trabalho e cota diária

**Files:** Modify `src-tauri/src/jev.rs` (`Client::evaluate` mede e chama
`usage::spend` com `jev:<set>`; lê `X-Jev-Calls-Used`/`X-Jev-Daily-Limit`),
`src-tauri/src/desktop/queue.rs` (vereditos de entrada/saída, economia de
barrado), `src-tauri/src/orchestrator.rs` (cache hit/miss, redações,
arquivos retidos, economia de contexto), `supabase/functions/jev/index.ts`,
migração `jev_count_call`.

- [ ] Testes: `evaluate` marca `jev:entry` com o uso devolvido; contagem de
  redações num contexto com marcadores.
- [ ] Implementar.

### Task 8: gravador, eventos e comandos no desktop

**Files:** Modify `src-tauri/src/desktop/mod.rs` (instala a pia, gravador),
`desktop/queue.rs` (`usage::within` em `serve` e no batismo),
`desktop/events.rs` (`usage-recorded`, `quota-changed`),
`desktop/commands/usage.rs` (`usage_report`, `refresh_quotas`).

- [ ] `cargo test` inteiro verde; `cargo clippy` sem avisos novos.

### Task 9: tela

**Files:** `package.json` (`recharts`); `src/modules/usage/{index,store}.ts`;
`src/modules/core/{bridge,types,bus}.ts`; `src/components/pages/StatsPage.tsx`;
organisms `UsageSummary`, `QuotaPanel`, `UsageCharts`, `JevPanel` de uso,
`UsageTables`; rodapé do chat, mini-resumo do projeto, rodapé do turno;
`Sidebar`, `AppHeader`, `App.tsx`; `en.ts`.

- [ ] `npm run typecheck` verde.

### Task 10: traduções, versão, verificação

**Files:** `supabase/migrations/20261001120600_usage_stats.sql` (tabelas,
RLS, `jev_count_call`, traduções nos 10 idiomas); versão 0.10.0 nos cinco
lugares.

- [ ] `cargo test`, `npm run typecheck`, `npm run build` (vite) verdes.
