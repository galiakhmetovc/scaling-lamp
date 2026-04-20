# App, CLI, and HTTP Cleanup Design

## Goal

Сделать `agentd` проще для дальнейшего развития daemon/background/cron/MCP без изменения поведения и без появления второго runtime path.

## Scope

Этот cleanup wave ограничен только слоями:

1. `cmd/agentd/src/cli.rs`
2. `cmd/agentd/src/bootstrap.rs`
3. `cmd/agentd/src/http/client.rs`
4. `cmd/agentd/src/http/server.rs`

Вне scope:

- `crates/agent-runtime/src/provider.rs`
- `cmd/agentd/src/execution/provider_loop.rs`
- `crates/agent-runtime/src/tool.rs`
- `cmd/agentd/src/tui/*` кроме минимального rewiring imports

## Invariants

- Канонический runtime path остаётся один.
- Prompt assembly order не меняется.
- TUI и CLI остаются thin over the same app/runtime layer.
- Daemon-backed CLI/TUI не получают special-case execution path.
- Поведение existing commands, HTTP routes и tests сохраняется.

## Problems To Fix

### `cli.rs`

Сейчас один файл держит:

- command parsing
- process-mode transport selection
- local command execution
- daemon-backed command execution
- interactive REPL
- rendering helpers
- terminal decoding helpers

Это уже мешает безопасно менять daemon/client UX.

### `bootstrap.rs`

`App` остаётся правильным фасадом, но конкретные session/chat/skills/context queries и mutations скопились в одном implementation file. Файл большой и плохо локализует ответственность.

### `http/client.rs` and `http/server.rs`

Статус, sessions, skills, chat turn, approvals и helpers сидят в одном месте на клиенте и на сервере. Для daemon growth это повышает цену любой правки.

## Target File Structure

### CLI

- `cmd/agentd/src/cli.rs`
  only public entrypoints and module wiring
- `cmd/agentd/src/cli/parse.rs`
  command parsing, global daemon connect options, process invocation parsing
- `cmd/agentd/src/cli/process.rs`
  process-mode dispatch, daemon-backed command selection, local-vs-remote routing
- `cmd/agentd/src/cli/repl.rs`
  generic REPL backend trait and chat repl implementation
- `cmd/agentd/src/cli/render.rs`
  string renderers for status/session/chat/skills/run/job/verification
- `cmd/agentd/src/cli/tests.rs`
  keep existing focused unit tests

### App / bootstrap

- `cmd/agentd/src/bootstrap.rs`
  `App`, `BootstrapError`, construction and module wiring only
- `cmd/agentd/src/bootstrap/session_ops.rs`
  session creation, listing, transcript, preferences, skills
- `cmd/agentd/src/bootstrap/context_ops.rs`
  session head, plan rendering, compaction, pending approvals
- `cmd/agentd/src/bootstrap/execution_ops.rs`
  mission tick, mission job execution, chat execution, approval continuation

### HTTP

- `cmd/agentd/src/http/client.rs`
  public module wiring only
- `cmd/agentd/src/http/client/status.rs`
- `cmd/agentd/src/http/client/sessions.rs`
- `cmd/agentd/src/http/client/chat.rs`
- `cmd/agentd/src/http/client/internal.rs`
- `cmd/agentd/src/http/server.rs`
  public routing entrypoint only
- `cmd/agentd/src/http/server/status.rs`
- `cmd/agentd/src/http/server/sessions.rs`
- `cmd/agentd/src/http/server/chat.rs`
- `cmd/agentd/src/http/server/internal.rs`

## Migration Rules

- Move code, do not redesign behavior.
- Keep type names stable unless a rename removes real ambiguity.
- Prefer extracting helpers verbatim first, then tightening imports.
- Keep integration tests as the main behavioral safety net.

## Verification

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo build -p agentd`
- `cargo build --release -p agentd`

