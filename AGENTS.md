# AGENTS.md

Instruções para agentes de código (Codex, Claude Code, Copilot etc.) que
trabalham neste repositório.

## O app é neutro

O JayV é um orquestrador genérico, no estilo do Codex: o Jev confere tudo que
o usuário envia e tudo que o LLM devolve (portaria de entrada e de saída) para
gastar menos tokens e evitar alucinação. Ele tem de servir para qualquer
projeto, de qualquer linguagem.

- Nunca coloque no código, nos testes, nos comentários, nos prompts ou nas
  migrações nomes, caminhos ou regras de um projeto específico do usuário.
  Exemplos e fixtures usam nomes genéricos (`src/lib.rs`, `util.py`,
  `web/src/main.ts`).

## Textos para pessoas: sempre pelo i18n

Toda mensagem que o usuário lê — rótulo, aviso, dica, erro — passa pelo i18n.
Nada de texto em português (ou em qualquer idioma) fixo no código.

- **Tela (`src/`)**: use `useT()` / `translate()` de `@/modules/i18n`. A chave
  nova nasce em inglês em `src/modules/i18n/messages/en.ts`.
- **Núcleo (`src-tauri/`)**: não escreve frase para pessoa nenhuma. Use
  `crate::i18n::Text` (`Text::new("chave").with("nome", valor)`; um valor pode
  ser outro `Text`). Ele é um erro: `bail!(Text::new(..))` atravessa o
  `anyhow`. O comando do Tauri devolve `Result<_, Text>` com
  `.map_err(i18n::failure)`, e o `reportError` de `@/modules/feedback` traduz
  com `say()`. Erro sem chave chega como `error.unexpected` com o motivo
  técnico.
- **Texto gravado no chat** (pedido barrado, falha, resposta a uma pergunta)
  vai como `i18n::notice(&[Text, ..])`; a tela o mostra com `shownText()`. O
  modelo lê o histórico pelo `i18n::for_model`, que diz o aviso em inglês.
- **Dado gravado** (fonte da leitura, tipo de saída, título padrão do chat) é
  identificador em inglês ou vazio, e a tela traduz. Mantenha a grafia antiga
  reconhecida na tela quando trocar um identificador já gravado.
- **Diagnóstico interno** (invariantes, protocolo do Jev, fila de sync) é em
  inglês: só aparece em log ou como motivo técnico de `error.unexpected`.
- **Traduções**: toda chave nova ganha uma migração em `supabase/migrations/`
  do repositório `JayV-Coder/supabase` (clonado ao lado deste), inserindo o
  texto em todos os idiomas de `public.locales` (pt-BR, en, es, zh-CN, hi,
  ar, fr, ru, ja, de), com `on conflict (locale, key) do update`. Mudar os
  parâmetros de uma chave já publicada é chave nova: o app antigo continua
  lendo a antiga.

Logs internos (`eprintln!`, `console.error`) e comentários de código não são
texto para o usuário e não precisam de i18n.

## Código: nomes sempre em inglês

Todo identificador — `fn`/`function`, `const`, `let`/variável, parâmetro,
`struct`/`enum`/`class`/`interface`/`type`, campo, módulo e nome de teste —
é escrito em inglês, no Rust e no React/TS/JS. Vale também para os testes
(`fn a_retry_reopens_the_same_turn`, não `fn retentar_reabre_o_turno`).

- Comentários e mensagens de `assert!` podem continuar em português.
- Dado já gravado em português (ex.: os tipos de saída `comando`/`arquivo`
  de checks antigos) não é identificador: fica como string entre aspas, só
  para a leitura do legado.
- Códigos de idioma (`ja`, `de`, `pt-BR`) não são palavras e ficam como estão.

## Instruções para o Jev e para os LLMs: sempre em inglês

Tudo que é escrito para um modelo ler fica em inglês, seja qual for o idioma
da conversa: perguntas e critérios do Jev (`jev.rs`, `gatekeeper.rs` e o
`*_seed_jev*.sql` do repositório `JayV-Coder/supabase`), prompts de sistema, instruções que vão
junto do pedido e identificadores gravados (ex.: os níveis de escopo
`small change` / `feature` / `whole system`). O modelo responde no idioma do
usuário; a instrução, não.

## Versão: sobe em todo commit no `main` e em todo PR para o `main`

O `release.yml` publica os instaladores na versão de
`src-tauri/tauri.conf.json`; enquanto ela não muda, cada build substitui os
instaladores do mesmo release e ninguém recebe a atualização. Por isso todo
commit que vai para o `main` — direto ou por PR — sobe a versão, nos cinco
lugares, sempre juntos:

- `package.json` e `package-lock.json` (as duas entradas da raiz)
- `src-tauri/tauri.conf.json`
- `src-tauri/Cargo.toml` e a entrada `jayv` do `src-tauri/Cargo.lock`

O número segue o [Versionamento Semântico](https://semver.org/lang/pt-BR/)
(`MAJOR.MINOR.PATCH`), escolhido pelo nível da modificação:

- **PATCH** (`0.7.0` → `0.7.1`): correção de falha que não muda o que já
  funcionava — bug, texto, tradução, ajuste visual.
- **MINOR** (`0.7.1` → `0.8.0`): funcionalidade nova compatível com o que
  existe — tela, comando, tabela ou coluna nova, opção nova. Zera o PATCH.
- **MAJOR** (`0.8.0` → `1.0.0`): mudança incompatível — dado gravado que
  deixa de ser lido, migração que exige ação, comando ou formato removido ou
  trocado. Zera MINOR e PATCH.

Um commit com mais de um nível sobe pelo maior. Enquanto o MAJOR for `0`, a
mudança incompatível sobe o MINOR e vem explicada na mensagem do commit.

O README do repositório público `jayv-coder-releases` é gerado pelo Actions
(`releases-readme.yml`, chamado ao fim do `release.yml`): não o edite à mão.

## Mensagem de commit: detalha cada alteração

A mensagem de todo commit no `main` começa por `vX.Y.Z: resumo` (a versão
nova) e, no corpo, detalha cada alteração em seções por assunto: o que mudou,
por quê e onde (arquivo ou módulo). Inclua as decisões que mudam o
comportamento para o usuário, as migrações novas e o que ficou de fora de
propósito. Ninguém deve precisar ler o diff para saber o que o commit faz.
