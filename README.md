# JayV

Jev is now a native Rust application with a Tauri 2 desktop interface and a CLI served by the same orchestration core. The former Python implementation is retained in `bkp/` only as migration evidence; it is not used at runtime.

## Requirements

- Rust 1.85 or newer
- Node.js 20 or newer
- Linux: GTK 3 and WebKitGTK 4.1 development/runtime packages, plus a running `xdg-desktop-portal` with a backend (`xdg-desktop-portal-gnome`, `-kde` or `-gtk`) — the folder picker goes through the portal

## Run

```bash
npm install
npm run dev
```

The desktop application opens when `jayv` is started without a subcommand. CLI commands remain available:

```bash
cargo run --manifest-path src-tauri/Cargo.toml -- status
cargo run --manifest-path src-tauri/Cargo.toml -- index
cargo run --manifest-path src-tauri/Cargo.toml -- run "Review the authentication module"
cargo run --manifest-path src-tauri/Cargo.toml -- version
```

Use `--config path/to/config.yaml` and `--root path/to/repository` to override discovery for CLI execution.

## Desktop workspace

The Tauri interface is the control plane for provider configuration. The shipped preset lists only the two CLI agents, `claude` and `codex`; every other provider and model is added by hand. Open **Configuração** to add, edit, enable, or remove OpenAI, Anthropic, OpenAI-compatible, and local CLI providers. API keys are masked after saving and are never sent back to the webview. On Unix, Jev writes the local configuration with owner-only permissions (`0600`).

For OpenAI, Anthropic, and compatible APIs, **Carregar modelos disponíveis** queries the provider through Rust. Changing the API key or compatible base URL also starts discovery automatically. A discovered model can then be added to the router without manually copying its ID.

Opening a project lands on its chat grid: one card per chat with the title, the creation date, the last gate pass recorded for it, and the most recent request sent in it. The gate log is stored in SQLite, so a card still shows the gate line for a chat that has not been touched in weeks, and the tally counts everything that ever crossed a gate rather than only what fits in the feed.

Every request sent opens a **turn**, identified by a short code the developer reads on both sides: the chat code, a middle dot, and the position of the request in that chat — `XY4T9B·04`. The prompt bubble carries that code, the entry-gate verdict, and a border in the verdict's colour; the answer bubble is bordered by what the exit gate found in it. Gate cards cite the same code, so a card in **Portaria** points back to one exact bubble.

A request is written to the database before any model is called, so leaving the chat, a failed send, or closing the app never removes it from the conversation. A turn still in flight when the app closes is marked failed on reopen. A failed prompt gets a **Reenviar** button under its bubble: the retry reuses the same turn, so the code does not change and the gate card is updated instead of duplicated. A prompt the entry gate blocked gets no such button — the refusal was deliberate.

Sending a message points the index at the folder of the project that owns the chat, so the context files, the project name in the prompt, and the exit scan all describe that repository and not the directory the app was launched from. The folder is re-indexed only when it actually changes, and a project without a folder falls back to the startup root. If the folder no longer exists, the send fails with that message instead of silently reading another repository.

Projects, chats, and messages are persisted in the local SQLite database `.jev/workspace.sqlite3`. The app starts with an empty workspace instead of creating a project from the executable directory. Each project can contain any number of chats, and each chat has isolated conversational context. Existing JSON history is imported once; the generated empty `src-tauri` workspace is intentionally ignored.

Chats can be deleted individually. Deleting a project also deletes all of its chats and messages through a database foreign-key cascade, after explicit confirmation in the UI. In the composer, `Enter` sends and `Shift + Enter` inserts a line break.

Assistant responses render common Markdown structures as native chat blocks: headings, lists, quotes, tables, inline code, links, and fenced code with language labels and copy controls.

Environment placeholders in existing YAML files remain supported for CLI compatibility, but desktop users do not need to edit YAML or `.env` files to configure providers.

In this project, the Jev model is intended to power the local control plane specified by [`JEV_V1.md`](JEV_V1.md) and [`JEV_V2.md`](JEV_V2.md): Jev classifies and scores structured routing decisions, while OpenAI, Anthropic, local models, and CLI agents execute the selected work.

The app calls Jev through the `jev` Edge Function of the Supabase project (`POST /functions/v1/jev`), signed in as the current user; the TypeSafe key lives only in that function's secrets. The questions and gate parameters live in the `jev_questions` and `jev_parameters` tables. Without a session, routing and the entry gate use the local heuristics.

## Architecture

- `src-tauri/src/orchestrator.rs`: intent, complexity, context, routing, execution, validation, and explanation pipeline.
- `src-tauri/crates/jayv-workspace/src/workspace.rs`: durable projects, chats, titles, and message histories.
- `src-tauri/crates/jayv-code/src/rag.rs`: repository indexing and lexical retrieval.
- `src-tauri/crates/jayv-base/src/firewall.rs`: deny/local-only policy and secret redaction.
- `src-tauri/crates/jayv-code/src/graph.rs`, `crates/jayv-agents/src/agents.rs`, `crates/jayv-code/src/context_engine.rs`: execution graph, specialist selection, context forks, and value scoring.
- `src-tauri/crates/`: the core layers that are already their own crates — `jayv-base` (i18n, config, context model, locked-down HTTP client, firewall, turn progress), `jayv-store` (local SQLite, sync outbox, context cache, task checkpoints), `jayv-cloud` (Supabase session and PostgREST), `jayv-agents` (LLM settings; OpenAI, Anthropic, OpenAI-compatible and subprocess CLI providers; router; permission-aware tools and sandbox; usage and quotas), `jayv-jev` (entry and exit gate, questions, expertise level, LLM policy, core settings, turns), `jayv-plans` (plan features), `jayv-orgs` (repository keys and checkouts), `jayv-code` (repository index, search, tree-sitter symbols, project map, execution graph, context engine) and `jayv-memory` (chat memory and project notes), `jayv-workspace` (projects, chats and the turn queue in local SQLite) and `jayv-live` (the live file panel). The layer order is in `src-tauri/src/layers.rs`.
- `src/`: Tauri desktop UI.

## Validation

```bash
npm run test
npm run build
```

`run` requires at least one configured and reachable provider. When no executable provider/model is configured, Jev returns local setup guidance without attempting an LLM request. Failures from a configured provider (for example, an unreachable endpoint or a rejected API request) are still returned as execution errors rather than synthetic success.
