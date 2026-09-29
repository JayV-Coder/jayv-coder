# Tempo real no chat e interações no box

Data: 2026-09-29 · Estado: aprovado para implementação

## Problema

Um pedido enviado no chat desaparece por dezenas de segundos. A tela mostra um
balão estático — `pendingBubble`, `src/main.js:141` — com o texto *"Analisando
intenção, contexto e rota…"*, que não muda quando a rota é escolhida nem quando o
modelo já está escrevendo. O desenvolvedor lê isso como travamento, e o trabalho
segue invisível no laço de fundo `serve_the_queue` (`src-tauri/src/lib.rs:180`).

Nenhum provedor faz streaming: `Provider::chat` (`providers.rs:13`) devolve a
resposta inteira, e `CliProvider` usa `child.wait_with_output()`
(`providers.rs:190`), que junta todo o stdout antes de retornar.

E quando a resposta do modelo é uma pergunta, ela chega como parágrafo solto. A
portaria já **manda** o modelo perguntar — `clarifying_note`
(`gatekeeper.rs:79`) injeta *"Faça uma única pergunta objetiva…"* quando o
veredito é `Ask` — mas responder exige redigir um pedido novo do zero.

## Objetivos

Os dois propósitos do app governam cada decisão abaixo:

1. Economizar **tokens e sessões** do LLM.
2. Forçar **objetividade** no prompt.

A interação estruturada serve os dois: um `SIM` clicado substitui um round-trip de
pergunta vaga, e uma escolha entre opções nomeadas é o prompt mais objetivo que
existe.

## Decisões tomadas com o usuário

| # | Decisão |
|---|---|
| 1 | Um spec só, cobrindo tempo real e interações, implementado em sequência. |
| 2 | Profundidade total: etapas do JEV + tokens do LLM + atividade dos agentes CLI. |
| 3 | Durável: log de etapas por evento, e a resposta parcial gravada com debounce. Nunca token a token no banco. |
| 4 | O JEV classifica a resposta do LLM e o retorno dele **habilita** a interação. A resposta do usuário volta pelo mesmo caminho: JEV, depois LLM. |
| 5 | Toda mensagem — texto, `SIM`, `NÃO`, escolha única, escolha múltipla — passa pela Portaria. O turno-resposta é julgado **em par com a pergunta** que o originou. Nenhum caminho dispensa o portão. |
| 6 | O box vira formulário: com pergunta pendente a caixa de texto está travada. `RESPONDER` é o que a destrava, em modo resposta. |
| 7 | Há saída: `IGNORAR` descarta a pergunta, com registro na Portaria. |
| 8 | O progresso sai do núcleo por um barramento de eventos por turno. O núcleo não conhece Tauri nem o banco. |

## Restrições descobertas no código

**O JEV não gera texto.** `Answer` (`jev.rs:52`) só devolve um número (`Noul`), uma
chave entre as opções enviadas (`Choice`) ou um nível (`Score`). Ele pode dizer
*"isto é uma pergunta de escolha múltipla"*, jamais *quais* são as opções. Logo o
texto das alternativas é extraído localmente e oferecido ao JEV como candidato —
o veredito do JEV segue sendo o que habilita a interação.

**`reqwest` está sem streaming.** `default-features = false, features = ["json",
"rustls-tls"]`. SSE exige a feature `stream` e `futures-util`.

> Corrigido na implementação: `Response::chunk()` existe sem a feature `stream`
> (verificado em reqwest 0.12.28). O SSE entrou sem dependência nova.

**O preset CLI não narra por igual.** `claude --model X --print` escreve texto
corrido no stdout; `codex exec` narra o próprio trabalho. A atividade de agente
vem de graça no codex e, no claude, só com `--output-format stream-json`.

## 1. Barramento, eventos e persistência

Módulo novo `src-tauri/src/progress.rs`.

```rust
pub enum Beat {
    Gate{verdict:String,score:u8,demand:u8},
    Read{intent:String,complexity:String,source:String},
    Context{files:usize,tokens:usize},
    Route{provider:String,model:String,reason:String},
    Running,
    Agent{line:String},
    Chunk{text:String},
    Done{input_tokens:usize,output_tokens:usize,latency_ms:u128},
    Failed{error:String},
}
```

`Pulse` embrulha `mpsc::Sender<Beat>`; `Pulse::silent()` serve o CLI e os testes,
e `beat()` engole erro de envio — canal fechado nunca derruba um turno, como o
`let _=app.emit(…)` que o `lib.rs` já usa.

`serve` abre o canal, passa o `Pulse` para `orchestrator.process` → `execute` → o
provedor, e sobe uma tarefa que drena o receptor com duas políticas:

- **Emite para a webview a cada evento**, inclusive cada `Chunk`. O IPC do Tauri é
  barato, e é isso que faz o texto crescer na tela.
- **Grava no SQLite com folga.** Beats raros (`Gate`…`Route`, `Done`, `Failed`) vão
  na hora; `Chunk` acumula e escreve `turns.partial` no máximo a cada ~400 ms ou
  2 KB. `Done`/`Failed` dão a descarga final.

```sql
CREATE TABLE IF NOT EXISTS turn_events (
  id TEXT PRIMARY KEY,
  turn_id TEXT NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
  at TEXT NOT NULL, seq INTEGER NOT NULL,
  kind TEXT NOT NULL, detail TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS turn_events_turn ON turn_events(turn_id,seq);
```

`detail` é JSON: o formato varia por `kind` e a tela só o lê de volta para
desenhar uma linha. Migração adicional no padrão de `workspace.rs:392`:
`ALTER TABLE turns ADD COLUMN partial TEXT`.

A consumidora toca **só** o banco, em trechos curtos, e nunca o orquestrador: a
ordem de cadeados documentada em `lib.rs:56` fica intacta.

`partial` é memória, não continuação. Nenhum provedor retoma uma resposta pela
metade; reabrir o app mostra o que havia chegado, marcado como interrompido, e a
fila refaz o turno — o primeiro `Chunk` novo sobrescreve o texto antigo.

## 2. Streaming nos provedores

O trait ganha um método com implementação padrão, para que nada quebre:

```rust
async fn chat_stream(&self,messages:&[ChatMessage],model:&str,pulse:&Pulse)->Result<ProviderResponse> {
    let response=self.chat(messages,model).await?;
    pulse.beat(Beat::Chunk{text:response.response.clone()});
    Ok(response)
}
```

`Orchestrator::execute` passa a chamar `chat_stream`. Um provedor que não
sobrescreva continua funcionando como hoje, emitindo um chunk só.

**HTTP.** `HttpProvider` sobrescreve com SSE: `"stream":true`, e para a OpenAI
`stream_options:{include_usage:true}`. As linhas `data:` são lidas de
`bytes_stream()`, com buffer de linha parcial entre pedaços de rede — um evento SSE
pode ser cortado no meio. OpenAI: `choices[0].delta.content`. Anthropic:
`content_block_delta` → `delta.text`, e `message_delta.usage` para os tokens.

**Retry e streaming.** `with_retry` (`providers.rs:105`) só pode agir **antes do
primeiro chunk**. Depois que texto foi emitido, uma quebra é falha, não nova
tentativa: repetir duplicaria o que já está na tela.

**CLI.** `CliProvider` troca `wait_with_output()` por leitura incremental:
`BufReader::lines()` no stdout e no stderr, em tarefas concorrentes, com o mesmo
`timeout` global de hoje. Regra de classificação, decidida por conteúdo e não por
configuração — assim não há campo novo nem mudança no preset:

- linha do **stderr** → `Beat::Agent`;
- linha do **stdout** que é JSON com um campo `type` reconhecido (`assistant`,
  `tool_use`, `result`, `content_block_delta`…) → roteada: texto do assistente vira
  `Chunk`, uso de ferramenta vira `Agent`;
- qualquer outra linha do stdout → `Chunk`.

Com isso o `codex exec` entrega atividade de agente sem tocar em nada, o `claude
--print` entrega texto progressivo, e quem ligar `--output-format stream-json` nos
args passa a ter atividade detalhada sem mudança de código.

## 3. O ciclo da pergunta

Módulo novo `src-tauri/src/asking.rs`.

**Extração local.** `extract(answer) -> Option<Candidate>` acha o enunciado (linha
ou frase final terminada em `?`) e a lista de opções imediatamente abaixo (`- `,
`* `, `1.`, `A)`), no máximo `MAX_CHOICE_OPTIONS`.

**Veredito do JEV.** `classify` chama `jev::evaluate` com duas perguntas:

- `kind`: `Question::choice` sobre `none | noul | single | multiple`;
- `options_are_real`: `Question::noul`, só quando houve lista extraída.

O `state` enviado leva a resposta do modelo e o candidato extraído. É este retorno
que habilita a interação.

**Reserva.** Sem `TYPESAFE_API_KEY`, ou quando a chamada falha, cai na heurística
local — lista presente vira `single`, enunciado sem lista vira `noul`, o resto vira
`none`. É o padrão que `entry_check` (`lib.rs:271`) já usa para o portão de entrada.

```sql
CREATE TABLE IF NOT EXISTS questions (
  turn_id TEXT PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
  at TEXT NOT NULL,
  kind TEXT NOT NULL,     -- noul | single | multiple
  prompt TEXT NOT NULL,
  options TEXT NOT NULL,  -- JSON
  source TEXT NOT NULL,   -- jev | heurística local
  status TEXT NOT NULL,   -- pending | answered | dismissed
  answered_by TEXT REFERENCES turns(id) ON DELETE SET NULL,
  settled_at TEXT
);
```

Uma pergunta por turno, e o `status` é a máquina de estados inteira. Só a mais
recente `pending` do chat trava o box.

## 4. Turno-resposta, Portaria e tela

**Envio.** `ProcessRequest` ganha `answers:Option<AnswerInput>` com
`{question_turn_id, kind, picked:Vec<String>, text:Option<String>}`.
`enqueue_prompt` confere que a pergunta está `pending`, compõe o texto do pedido
de forma determinística — `Resposta à pergunta «…»: SIM` / `…: opção A, opção C` /
`…: <texto livre>` — grava o vínculo e marca a pergunta como `answered`.

**Portaria em par.** O texto pontuado pelo portão de entrada é o **par composto**:
o pedido do turno que perguntou, mais a pergunta, mais a resposta. Um `SIM` sozinho
seria barrado pelos critérios de `judge` (`gatekeeper.rs:124`); o par herda
objetivo e critério de pronto do turno de origem e é julgado com substância. Nenhum
critério novo, nenhum desconto de `demand`.

**Contexto e tokens.** O turno-resposta **não** remonta contexto às cegas: a
consulta de recuperação é o par composto, que carrega o pedido original, e o
histórico limitado por `HISTORY_MESSAGES` já traz o turno anterior. Sem cache novo
e sem contexto persistido.

**Comandos novos.** `answer_question` e `dismiss_question`. A pergunta pendente
entra no `WorkspaceData` do chat, para a tela desenhá-la a partir do banco como
desenha todo o resto.

**Tela.** `src/main.js` ganha ouvintes de `turn-beat` e `turn-chunk` que atualizam
o balão em aberto **sem** o `refresh()` inteiro — full redraw continua só no
`turn-settled`. O balão pendente passa a ter uma lista de atividade e o texto
crescendo. O composer ganha três trajes:

- livre: como hoje;
- `noul`: caixa travada, `SIM` · `NÃO` · `RESPONDER` · `IGNORAR`;
- `single`/`multiple`: caixa travada, opções listadas; escolha única envia ao
  clicar, múltipla marca e envia; `IGNORAR` sempre presente.

`RESPONDER` destrava a caixa em modo resposta — o texto passa a significar
resposta àquela pergunta, e não pedido novo.

## Testes

No estilo da casa (unitários e auto-verificações com `include_str!`):

- extração de enunciado e opções, incluindo os quatro formatos de lista;
- heurística de reserva quando o JEV não está configurado;
- buffer de linha SSE cortada entre pedaços de rede;
- retry só antes do primeiro chunk;
- debounce: N chunks geram no máximo uma escrita por janela;
- máquina de estados da pergunta: `pending` → `answered` / `dismissed`, e a recusa
  de responder duas vezes;
- pontuação do par composto contra um `SIM` cru.

## Ordem de implementação

1. `progress.rs`, schema, fiação no `serve`, beats de etapa. Já mata o silêncio.
2. Streaming nos provedores: SSE e CLI incremental.
3. Tela ao vivo: ouvintes, balão crescendo, lista de atividade.
4. `asking.rs`, tabela `questions`, classificação pelo JEV.
5. Turno-resposta, par na Portaria, trajes do composer.

## Fora de escopo

- Retomar geração interrompida (nenhum provedor suporta).
- Botão de cancelar turno em voo.
- `Question::Score` como interação de tela.
- Mudar os args do preset em `config.yaml`.
