# Latência, CLIs e recursos obrigatórios — plano de implementação

**Goal:** o pedido chega mais rápido à tela, a integração com as CLIs deixa de
falhar nos casos medidos e o plano mantém ligado o que protege o usuário.

**Architecture:** ver `docs/superpowers/specs/2026-10-05-latencia-e-recursos-design.md`.

**Tech Stack:** Rust (tokio, rusqlite, ignore), Deno (edge function), Postgres/pgTAP, React 19, Next.js 16, Vitest.

## Global Constraints

- Identificadores em inglês; texto de tela só pelo i18n, com migração nos 10 idiomas.
- Cada onda sobe a versão nos cinco lugares (onda 1: 0.59.1; onda 2: 0.60.0; onda 3: 0.61.0).
- O que funciona continua funcionando: a resposta gravada do agente não muda de forma.
- Recurso de núcleo nunca fica desligado — nem offline, nem com servidor antigo.

## Review Focus

1. Claude com `--include-partial-messages`: a resposta gravada é igual à de antes; o texto aparece antes do fim da mensagem.
2. Jev fora do ar: a portaria decide pela heurística em no máximo `deadline_seconds`; depois de três falhas, sem esperar.
3. Repetição de uma chamada ao Jev conta uma vez no limite diário.
4. `.venv` gigante não tira `src/` do índice.
5. Recurso travado no plano não desliga pela tela, pelo banco local nem offline.

---

## Onda 1 — 0.59.1

### Task 1: Saída das CLIs (`jayv-agents/providers.rs`)
- [x] Parse único por linha; `Meter::read_event`.
- [x] `text_delta` do Claude vira fala; o `assistant` seguinte fecha a mensagem sem duplicar (L4).
- [x] `system` fora do `init`, `active_goal` e `autocompact_state` são escrituração (B4).
- [x] Leitura em bytes com `from_utf8_lossy` (B5).
- [x] Entrada padrão escrita em paralelo à leitura (B6).
- [x] `provider.failed` com `origin` (B8).

### Task 2: Orquestrador (`jayv-orchestration`)
- [x] `lost_session` reconhece a sessão perdida do Codex (B1).
- [x] Janela do plano B por tentativa (B7).
- [x] `could_not_start` ignora motivo vindo só da resposta (B8).

### Task 3: Jev (`jayv-jev/jev.rs`)
- [x] Cliente HTTP único, prazo por pedido, `deadline_seconds`, uma repetição em `entry`/`routing`.
- [x] Disjuntor e `reachable()`; `daily_limit` abre até a meia-noite UTC.
- [x] Cabeçalho `x-jev-request` estável nas repetições.

### Task 4: Índice (`jayv-code/rag.rs`)
- [x] Varredura pelo crate `ignore` com as pastas geradas de fora (B12).
- [x] Orçamento por prioridade; arquivo que não cabe é pulado.

### Task 5: App (`src-tauri/src/desktop`)
- [x] `SharedForget`: apagar e limpar sem esperar o orquestrador (L12).
- [x] Portaria e roteamento usam `jev::reachable()`.
- [x] O narrador junta os pedaços num quadro de 50 ms (`progress::Frame`): o streaming do Claude não vira um redesenho por token.

### Task 6: Edge function e migração (`supabase`)
- [x] `jev_calls` + `jev_count_call(request)` idempotente; `jev_refund_call(request)`.
- [x] `fetch` com prazo, devolução no estouro, leitura paralela, cache das perguntas.
- [x] `deadline_seconds` em `jev_parameters`.
- [x] pgTAP.

## Onda 2 — 0.60.0

### Task 7: Recursos obrigatórios (`supabase`, `site`, app)
### Task 8: Cancelar e teto total
### Task 9: Privacidade nas CLIs (S1, S2)
### Task 10: Índice por pasta e tela por chat (L5, L6)
### Task 11: Vigia, revisão, fila sem sync, login das CLIs (L7, L8, L9, B9)
### Task 12: Custo e tela (L10, L11, B2, B3, B11)

## Onda 3 — 0.61.0

### Task 13: `TurnContext`, `Shared` e o semáforo por plano (L1)
