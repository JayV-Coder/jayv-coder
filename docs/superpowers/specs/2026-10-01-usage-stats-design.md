# Estatísticas de uso — design

Ciclo 1 de 2 (o ciclo 2 é o sistema de notificações, que consome os eventos
`usage-recorded` e `quota-changed` criados aqui).

## Objetivo

Mostrar, com precisão declarada, quanto o JayV gastou e quanto o Jev
trabalhou, separado por chat, por projeto e no total (global, somando todos
os computadores do usuário):

- tokens de entrada, saída e cache de cada chamada a LLM (agentes CLI,
  provedores HTTP e o próprio Jev);
- limite do plano de cada CLI (janela de sessão, semanal, mensal) e
  atividade de cada agente (execuções, duração, sucesso/falha);
- desempenho do Jev: trabalho (vereditos, cache, firewall), custo (tokens e
  tempo por etapa, cota diária) e economia estimada.

## Regras de precisão

- Todo número carrega `precision`: `reported` (a ferramenta informou),
  `estimated` (o app calculou) ou `legacy` (turnos antigos, estimados por
  caracteres/4).
- Custo em USD só aparece quando a ferramenta informa (Claude
  `total_cost_usd`, rotulado "equivalente na API"); nos demais casos a tela
  mostra "—". Nada de tabela de preços própria.
- Economia do Jev é sempre `estimated`, mostrada à parte e nunca somada aos
  números reais.
- Quando o período mistura `estimated`/`legacy`, a tela mostra um selo
  ("inclui X% estimado").
- Tokens contam só o que passou pelo JayV; o limite do plano é da conta
  inteira (inclui uso fora do JayV) e a tela diz isso.

## Coleta

Toda fonte produz um `UsageRecord` e o envia a um gravador único,
`usage::Ledger` (`src-tauri/src/usage/`). Nenhum outro módulo grava as
tabelas de uso.

- **Claude** (`--output-format stream-json`): o evento `result` fornece
  `usage`, `modelUsage` (um registro por modelo, cobre subagentes) e
  `total_cost_usd` → `reported`. O `rate_limit_event` deixa de ser
  descartado e dispara uma leitura do limite.
- **Codex**: passa a rodar `exec --json`. `turn.completed.usage` (input,
  cached_input, output) → `reported`; `rate_limits` → `quota_snapshots`. O
  texto do chat passa a ser extraído dos eventos JSON.
- **Copilot**: `--silent` sai; entram `--output-format json` e
  `--usage-output-file <tmp>`. O arquivo é lido no fim da execução →
  `reported`. Formato desconhecido cai para estimativa `estimated`.
- **HTTP**: o uso real já lido em `providers.rs` passa a gerar o registro.
- **Jev**: as quatro chamadas (`entry`, `route`, `verify`, `ask`) geram
  registro com o `Usage` devolvido pela função e a duração. A função `jev`
  passa a responder com os cabeçalhos `X-Jev-Calls-Used` e
  `X-Jev-Daily-Limit` (a contagem vem de `jev_count_call`, que devolve o
  número em vez de um booleano); o app guarda a última leitura em memória.
- **Trabalho do Jev**: tudo vai para `jev_records`: `entry:pass|ask|block`,
  `exit:cleared|held`, `cache_hit`, `cache_miss`, `secret_redacted`,
  `file_withheld`, `saved_tokens:blocked|cache|context`. Os vereditos também
  ficam em `entry_checks`/`exit_checks`, mas essas tabelas somem em cascata
  quando o chat é apagado; o histórico das estatísticas não pode sumir junto.
- **Economia estimada**: barrado = tokens do prompt + média de saída do
  modelo roteado; cache = tokens da resposta reaproveitada; contexto =
  tokens dos arquivos cortados.
- Falha de coleta nunca derruba o turno: grava `estimated` e loga o motivo.
- `.jev_performance.json` continua só para o roteamento.

## Armazenamento e sync

Tabelas novas no SQLite do usuário, espelhadas no Supabase pelo outbox
(`local/outbox.rs`, `TABLES`) e por uma migração com `user_id`,
`row_updated_at`, `row_deleted_at`, `synced_at` e RLS:

- `usage_records(id, project_id, chat_id, turn_id, source, model,
  input_tokens, output_tokens, cache_read_tokens, cache_write_tokens,
  cost_usd, duration_ms, success, precision, machine_id, created_at)`.
  `source`: `claude`, `codex`, `copilot`, `http:<provedor>`,
  `jev:entry|route|verify|ask`.
- `quota_snapshots(id, agent, window, used_percent, resets_at, plan,
  captured_at, machine_id)`. `window`: `session`, `week`, `month`. Leitura
  igual à anterior em menos de 1 min é descartada.
- `jev_records(id, project_id, chat_id, turn_id, kind, amount, precision,
  created_at)`.

Índices em `(created_at)`, `(project_id, created_at)`, `(chat_id,
created_at)`. As linhas são só acrescentadas: não há conflito entre
máquinas. Apagar chat/projeto não apaga o uso; a tela mostra "(chat
removido)". A migração local copia os eventos `done` de `turn_events` como
`legacy`.

`usage::query(scope, period)` (`Global | Project(id) | Chat(id)`) agrega em
SQL: totais, série diária (fuso local), quebra por fonte/modelo e por
projeto/chat, último limite por agente, resumo do Jev. Comando Tauri:
`usage_report`.

## Limite do plano

- **Claude**: `claude -p "/usage"` (não gasta tokens). Extrai "Current
  session: N% used · resets …" e "Current week (…): N% used · resets …". Se
  o texto não casar, estado "indisponível" com motivo e a última leitura boa
  continua visível com a idade.
- **Codex**: `rate_limits` dos eventos `--json`; sem execução recente, o
  último `rate_limits` de `~/.codex/sessions/**/*.jsonl` (só leitura).
- **Copilot**: premium requests e tokens por execução (do arquivo de uso),
  somados por mês. Saldo do plano: "não informado pelo Copilot CLI". O
  endpoint interno `copilot_internal/user` fica fora de propósito.
- Quando lê: ao abrir o app; após `rate_limit_event` (mínimo 2 min entre
  leituras); a cada 15 min com a tela de Estatísticas aberta; no botão
  "atualizar". Timeout de 30 s; CLI ausente ou sem login é pulado com
  motivo.
- Ao cruzar um patamar (80%, 95%), o núcleo emite `quota-changed`.

## Telas

- **Estatísticas** (`View = "stats"`, item na barra lateral): seletor de
  escopo e de período (Hoje, 7d, 30d, Tudo, intervalo); cartões (tokens,
  custo informado, turnos, taxa de sucesso); barras de limite por agente e
  janela; gráficos (`recharts`) de tokens por dia por fonte e por modelo;
  painel do Jev (trabalho, custo, economia estimada com dica de cálculo);
  tabelas por projeto, chat e modelo, com clique que filtra o escopo.
- **Em contexto**: rodapé do chat, mini-resumo de 30 dias no cartão do
  projeto, tokens e duração no rodapé de cada turno (ícone de estimado).
- Atualiza ao vivo com `usage-recorded` e `quota-changed` (debounce).
- Textos pelo i18n (chaves em `en.ts` + migração nos 10 idiomas); números e
  datas com `Intl`.

## Testes

- Rust: parsers com fixtures (Claude `result`/`rate_limit_event`/`/usage`,
  inclusive formato inesperado; Codex `turn.completed`/`rate_limits`;
  arquivo de uso do Copilot); `usage::query` com SQLite em memória
  (escopos, períodos, precisões mistas, chat removido); migração `legacy`;
  tabelas novas no outbox.
- Frontend: formatação de "—" e de estimado.

## Fora de escopo

- Notificações (ciclo 2).
- Tabela de preços própria e custo estimado.
- Saldo mensal do Copilot via endpoint não documentado.

## Versão

v0.10.0 (funcionalidade nova), nos cinco lugares.
