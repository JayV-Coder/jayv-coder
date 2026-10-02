# Política de LLM — plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a organização (e cada repositório dela) define com quais agentes e modelos os projetos rodam, desliga os modos sem trava dos agentes e endurece privacidade e portaria de saída; o app aplica a política efetiva a cada pedido.

**Architecture:** tabela nova no Supabase com leitura por RLS e escrita só por RPC; a junção organização + repositório é feita no banco (`llm_policy_of`) e entregue por `my_project_policies`; a sincronização guarda o resultado num cache local; o atendimento passa as configurações de quem usa pela política antes de rotear; o React edita a política na página da organização.

**Tech Stack:** Postgres/pgTAP, Rust (rusqlite), React 19, zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-02-politica-llm-design.md`

## Global Constraints

- Identificadores em inglês; texto de tela só pelo i18n, com tradução nos 10 idiomas na migração.
- A política só aperta: nenhum campo liga o que quem usa desligou nem afrouxa regra.
- Escrita na tabela só por RPC; leitura só para membro.
- Falha ao baixar a política não para a sincronização; o cache anterior vale.
- Versão 0.18.0 → 0.19.0 nos cinco lugares.

## Review Focus

1. Política de repositório de outra organização → `policy.repository` (Task 1).
2. Organização lista `claude, codex` e repositório lista `codex, cursor` → só `codex` (Task 1).
3. Política ilegível no cache → vale a mais rígida, não nenhuma (Task 2).
4. Servidor sem a migração → a volta da sync segue e a tela de organizações abre (Tasks 3 e 5).
5. Política deixa o pedido sem agente → orientação `guidance.policyBlocked` com `@slug` (Task 4).

---

### Task 1: Migração e pgTAP (`supabase`)
- [x] `tests/llm_policies.test.sql`: papéis, recusas, leitura, junção, `my_project_policies` com fork.
- [x] `migrations/20261002121000_llm_policies.sql`: tabela, RLS, `set_llm_policy`, `clear_llm_policy`, `llm_policy_of`, `project_repository`, `my_project_policies`, traduções.
- [x] Rodar as migrações e o pgTAP num Postgres 16 local com `auth` de mentira.

### Task 2: `policy.rs` (Rust, TDD)
- [x] `LlmPolicy::restrict_llm` e `restrict_core`; `AgentSettings::without_unsafe_modes` no `llm.rs`.
- [x] Cache `project_policies`: `replace_all`, `for_chat`; política ilegível é a mais rígida.

### Task 3: Sincronização
- [x] `Backend::project_policies` (padrão `None`), `Remote` chama `rpc/my_project_policies`.
- [x] `sync::round` troca o cache depois de baixar as tabelas; falha só vai ao log.

### Task 4: Atendimento
- [x] `queue::apply_project_policy` antes de focar a pasta do chat.
- [x] `Orchestrator::policy_scope` e as orientações `guidance.policyBlocked` / `guidance.policyFix`.

### Task 5: Telas
- [x] `policy.ts` (regras puras, Vitest) e as RPCs no módulo `organizations`.
- [x] Aba **Política de LLM** (`OrganizationPolicy`), selo no card do projeto.

### Task 6: Textos, verificação e entrega
- [x] Chaves no `en.ts` e traduções na migração (10 idiomas).
- [x] `npm run test:web`, `npm run typecheck`, `npm test` (cargo), build.
- [x] Versão 0.19.0; PR nos dois repositórios.
