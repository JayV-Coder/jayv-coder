# Latência, CLIs e recursos obrigatórios — design

Resposta técnica à auditoria de 5 de outubro (latência entre as
funcionalidades e as CLIs dos provedores, bugs de integração e recursos que o
plano tem de manter ligados). Os códigos `L*`, `B*`, `S*` e `F1` são os da
auditoria. A entrega vem em três ondas, cada uma um PR por repositório:

| Onda | Versão | Natureza | Itens |
|---|---|---|---|
| 1 | 0.59.1 | correções, sem mudar o que funciona | L3, L4, L12, B1, B4, B5, B6, B7, B8, B12, S4 |
| 2 | 0.60.0 | funcionalidade nova compatível | F1, S1, S3, S5, L2, L5, L6, L7, L8, L9, B9 e o resto |
| 3 | 0.61.0 | arquitetura da fila | L1 |

## Princípios

- **O pedido nunca espera o que não é dele.** Rede do Jev, sincronização,
  batismo, revisão e vigia de arquivos têm prazo próprio e rodam fora do caminho
  que leva o texto do agente à tela, ou com orçamento curto e reserva local.
- **Falha de serviço auxiliar degrada, não para.** Sem Jev, heurística; sem
  sincronização, a fila anda com a sessão válida; sem lista de recursos, o
  plano padrão embutido — e os recursos obrigatórios ligados.
- **O que protege não se desliga.** Redação de segredos, arquivos sensíveis,
  sessões do agente, cache de contexto, roteamento e portarias são núcleo: o
  plano os trava ligados, e o próprio app aplica a trava sem depender do
  servidor.
- **Um parse por linha.** A saída do agente é lida como JSON uma vez; cada
  leitor (conta, sessão, recusa, fala, relato) recebe o mesmo `Value`.

## Onda 1

### L4 — O texto do Claude enquanto ele escreve

O Claude já roda com `--include-partial-messages`; os `stream_event` com
`content_block_delta`/`text_delta` passam a ser fala. A mensagem inteira
(`assistant`) que chega depois deles não é somada de novo:

- o primeiro `text_delta` de uma mensagem abre o balão (a quebra
  `MESSAGE_BREAK`, se já havia texto) e cada pedaço vai para a tela e para a
  resposta;
- o `assistant` com texto fecha a mensagem: se a prévia bate com o texto final,
  só a quebra de linha do fim entra; se não bate (o agente corrigiu algo no
  meio), a resposta gravada troca a prévia pelo texto final — a tela se acerta
  no `turn-settled`, que relê a mensagem do banco;
- `thinking_delta` e `input_json_delta` continuam escrituração.

A resposta gravada fica idêntica à de hoje; muda só quando a tela a vê.

### L3 — O Jev com orçamento

- Um `reqwest::Client` só (`OnceLock`), sem prazo global; o prazo vai em cada
  pedido.
- `entry` e `routing` correm com um prazo total (parâmetro `deadline_seconds`
  de `jev_parameters`, padrão 8 s, entre 2 e 30) e no máximo uma repetição. O
  `asking`, que roda depois da resposta e fora da fila, fica com 30 s e quatro
  tentativas.
- Disjuntor: três falhas seguidas de rede, prazo ou 5xx abrem o circuito por 5
  minutos; enquanto aberto, `jev::reachable()` é falso e a portaria e o
  roteamento vão direto à heurística, sem esperar. Um sucesso fecha. O limite
  diário (`daily_limit`) abre o circuito até a meia-noite UTC.
- Cada pedido leva o cabeçalho `x-jev-request` (UUID), o mesmo nas
  repetições: a função conta a chamada uma vez só (S4).

### S4 — Edge function `jev`

- `jev_count_call(request_id)` grava o id numa tabela de chamadas do dia e só
  soma quando ele é novo; a versão sem argumento continua existindo para o app
  antigo.
- O `fetch` à TypeSafe tem prazo (`JEV_UPSTREAM_TIMEOUT_MS`, padrão 25 s,
  abaixo dos 30 s do app antigo) e a chamada é devolvida também no estouro.
- A contagem e a leitura das perguntas saem em paralelo; as perguntas ficam em
  memória no isolate por 5 minutos.

### B1, B4, B5, B6, B7, B8 — A conversa com a CLI

- **B1** `lost_session` reconhece `no rollout found` e `thread/resume failed`
  (frase real do Codex 0.160).
- **B4** `system` com qualquer subtipo que não `init` é escrituração (a tela
  dizia "agente começou" de novo a cada um); `active_goal` e
  `autocompact_state` entram na lista.
- **B5** As duas pontas são lidas em bytes (`split(b'\n')`, que é cancel-safe
  como o `next_line`) e convertidas com `from_utf8_lossy`.
- **B6** O pedido é escrito na entrada padrão por uma tarefa própria, ao mesmo
  tempo que a saída é lida; o erro que não for `BrokenPipe` só derruba o pedido
  se o agente também falhar.
- **B7** A janela do plano B conta a partir de cada `execute`, não do começo
  do pedido: um `planFirst` de minutos não tira mais o plano B do construtor
  sem login.
- **B8** O motivo do `provider.failed` diz de onde veio (`origin`:
  `announced`, `stderr`, `output`); `could_not_start` ignora o que veio só do
  texto da resposta.

### B12 — O índice que vê o projeto

- A varredura usa o crate `ignore`: respeita `.gitignore` (mesmo sem `.git`),
  mantém os arquivos ocultos que não estão ignorados e pula, além da lista de
  hoje, `.venv`, `venv`, `.next`, `.nuxt`, `coverage`, `vendor`, `.tox`,
  `.mypy_cache`, `.pytest_cache`, `.turbo`, `.gradle`.
- O orçamento de 48 MB é gasto por prioridade: código, depois configuração,
  depois documentação, depois JSON; um arquivo que não cabe é pulado, não
  interrompe a leitura.

### L12 — Apagar e limpar sem esperar o agente

`clear_chat`, `delete_chat` e `delete_project` só pegam o banco. A limpeza da
memória do orquestrador vai para uma lista de esquecimento
(`SharedForget`): aplicada na hora se o orquestrador está livre, ou no começo
do próximo atendimento.

## Onda 2

### F1 — Recursos obrigatórios por plano

Banco (`supabase`):

| Objeto | Mudança |
|---|---|
| `features.core` | `boolean`, `true` para `secretRedaction`, `sensitiveFiles`, `agentSessions`, `contextCache`, `adaptiveRouting`, `entryGate`, `exitGate` |
| `plan_features.mode` | `optional` ou `locked` |
| `plan_features.default_on` | o valor de partida do opcional |
| `plans.jev_daily_limit`, `plans.max_concurrent_turns` | limites do plano |
| `admin_set_feature` | recusa desligar recurso de núcleo (`admin.error.coreFeature`) |
| `admin_save_plan` | aceita `features` como chaves (formato antigo) ou objetos `{key, mode, default_on}`; núcleo entra sempre `locked` |
| `my_features()` | soma `locked`, `defaults`, `limits` sem mexer em `features` |
| função `jev` | limite do plano de quem chama, `JEV_DAILY_LIMIT` como reserva |

App:

- `Entitlements` guarda `features`, `locked`, `defaults` e `limits`.
  `enforce_core` liga o que é travado (`redact_secrets`, `deny` ⊇ padrão,
  `cache_ttl ≥ 300`, `keep_session_model`, `adaptive_routing`) e
  `enforce_llm` liga `persist_sessions` no Claude. Rodam depois da política da
  organização e antes de o orquestrador receber as configurações.
- Sem cache, os recursos de núcleo valem travados e os outros seguem o plano
  padrão embutido (`FREE_DEFAULT`), em vez de "vale tudo" (S3).
- A tela mostra o travado desligável como desabilitado, com "obrigatório no
  seu plano".
- O núcleo passa a respeitar `liveFiles`, `answerRecall`, `projectNotes` e
  `symbolIndex`, que hoje só a tela respeita.

Site: catálogo com as chaves novas; aba Recursos com cadeado no núcleo;
editor de plano com três estados (Fora, Opcional, Obrigatório), valor de
partida e os dois limites.

### L2 — Cancelar e teto total

- `CancelRegistry` (turno → `CancellationToken`) no estado do app; comando
  `cancel_turn(turn_id)`. O atendente passa o token ao orquestrador, que o
  repassa a cada `chat_turn`.
- `CliProvider` corre o processo num grupo próprio (`process_group(0)` no
  Unix, `CREATE_NEW_PROCESS_GROUP` no Windows) e mata o grupo inteiro no
  cancelamento, no estouro de silêncio e no teto total (B10).
- Teto total por agente (`max_minutes`, padrão 30, entre 5 e 240) ao lado do
  prazo de silêncio.
- Turno cancelado fecha como `failed` com o aviso `turn.cancelled` e ganha o
  "Reenviar" de sempre.

### S1, S2, S5 — Privacidade que chega às CLIs

- Claude: cada padrão de `deny` vira `Read(<padrão>)`, `Edit(<padrão>)` e
  `Write(<padrão>)` no `--disallowedTools`.
- Codex e Cursor não têm regra por caminho: a tela avisa; com
  `sensitiveFiles` travado e arquivo negado presente na pasta, eles saem do
  roteamento daquele projeto.
- S2: o vigia de arquivos cruza o que mudou com a regra de escrita e a lista
  `deny`; arquivo protegido alterado vira um `ExitCheck` segurado com o
  caminho.
- S5: `redact_secrets` travado (F1).

### L5 — Índice por pasta, incremental

`IndexPool` guarda até três `RepositoryRag` por pasta (LRU). Reindexar compara
data e tamanho de cada arquivo e só relê o que mudou; a varredura roda em
`spawn_blocking`.

### L6 — A tela por chat

`get_workspace` devolve projetos e a lista de chats sem mensagens;
`get_chat(chat_id)` devolve mensagens, turnos e pergunta de um chat. Os
eventos `chat-prompt`, `turn-settled` e `chat-renamed` fazem a tela reler só o
chat afetado.

### L7, L8, L9, B9, L10, L11

- **L7** O vigia só roda com `liveFiles`; o `git status` é chamado sem o
  cadeado das sessões.
- **L8** A revisão (segunda opinião) só para `medium` e `complex`, e corre
  depois da resposta, num bloco próprio do mesmo turno; o plano do `planFirst`
  é transmitido.
- **L9** A fila anda com sessão válida mesmo `Offline`; a sincronização segura
  as voltas enquanto há turno no ar.
- **B9** `codex login status` e `claude auth status` em cache (10 min); agente
  sem login sai do roteamento com aviso.
- **L10** O caminho do executável fica em cache por comando; provedores só
  são reconstruídos quando as configurações mudam; o título não usa CLI.
- **L11** Seletor por turno no `useConversation` e pedaços juntados por
  quadro.

### B2, B3, B11

- **B2** `manual` no lugar de `default` (lido o antigo), `dontAsk` oferecido e
  `--permission-prompts none` sempre.
- **B3** `safeMode` e `symbolTools` não convivem.
- **B11** Chamadas de uma vez só (plano, revisão, divisão) com
  `--no-session-persistence`.

### Decisões tomadas na implementação (0.60.0)

O que mudou em relação ao desenho acima, e por quê:

- **S3** Sem cache do plano, só o núcleo vale travado; os outros recursos
  seguem como antes (liberados até a primeira lista). Um `FREE_DEFAULT`
  embutido tiraria recursos de quem paga no primeiro arranque offline.
- **L2** O teto é um ajuste só, `turn_ceiling_minutes` nas configurações do
  Jev (padrão 30, entre 5 e 240), e conta o pedido inteiro — portaria, plano,
  agente e revisão —, não cada agente. O sinal de parar viaja no `Pulse`
  (`progress::Stop`): as chamadas de apoio usam `pulse.quiet()` e param
  junto. O texto que o agente já tinha dito fica no chat antes do aviso.
- **S1** O Claude recebe `Read(<padrão>)` e `Edit(<padrão>)`. Regra de
  caminho em `Write` o Claude aceita e nunca consulta (a documentação de
  permissões manda usar `Edit`), então ela não entra.
- **S1** Codex, Copilot e Cursor não saem do roteamento quando há arquivo
  protegido na pasta: quase todo projeto tem um `.env`, e quem só tem o
  Codex ficaria sem agente. A proteção deles é o firewall (o arquivo não vai
  no contexto) e o S2, que segura na saída qualquer protegido que o agente
  mexer; a tela de privacidade diz isso.
- **S2** O vigia não serve: o `git status` não lista o que está no
  `.gitignore`, e é o caso do `.env`. A portaria tira uma foto dos protegidos
  (tamanho, data e, nos pequenos, a impressão do conteúdo) antes do pedido e
  compara depois, com a saída de tipo `changed`.
- **L6** Entrou o `get_chat(chat_id)` e a tela relê só o chat do aviso
  (`chat-prompt`, `turn-settled`, envio). O `get_workspace` sem mensagens
  fica para a onda 3: os cartões e a lateral leem os turnos do retrato, e
  tirar as mensagens pede campos de resumo que ainda não existem.
- **L8** A revisão sai do pedido como `ReviewRequest` e corre depois da
  resposta, presa à pasta daquele pedido. O plano do `planFirst`
  transmitido ficou de fora desta versão.
- **L10** O título continua com o agente de linha de comando no modelo mais
  barato, agora sem guardar sessão (B11) e fora do cadeado. Tirar o agente
  deixaria todo chat com o resumo local do primeiro pedido.
- **B2** As flags novas (`--permission-prompts`, `--ephemeral`) passam por
  `llm::understood`: a ajuda do executável instalado diz se ele as conhece;
  se não, saem da linha de comando em vez de derrubar o pedido.
- **B9** O login é perguntado em segundo plano (`claude auth status`,
  `codex login status`) e vale 10 minutos; a conferência do agente na tela
  pergunta na hora.

## Onda 3 — L1, pedidos em paralelo

O `Orchestrator` vira dois pedaços:

- `Shared` (`Arc`): desempenho, memória dos chats, cache semântico, planos e
  reclamações por chat, `IndexPool`.
- `TurnContext` (por pedido): configuração já com política e plano, provedores
  e planejadores, firewall, `Workdir` próprio, o índice da pasta e os
  `pending_*` de hoje.

O atendente monta o `TurnContext` com o cadeado do banco, solta tudo e chama
`process(ctx, &shared, pulse, cancel)`. Pedidos de chats diferentes correm em
paralelo até `max_concurrent_turns` do plano (semáforo); pedidos do mesmo chat
continuam em ordem.

## Compatibilidade

- App antigo com servidor novo: `my_features().features` não muda; o app antigo
  só não aplica a trava.
- App novo com servidor antigo: sem `locked`, o app trava o núcleo pela lista
  embutida.
- Nenhum dado gravado deixa de ser lido: `permissionMode = default` é lido como
  `manual`.

## Testes

Cada item ganha teste no crate em que mora, com nome em inglês, além do
pgTAP para o banco e do Vitest para a tela. A lista está no plano
(`docs/superpowers/plans/2026-10-05-latencia-e-recursos.md`).
