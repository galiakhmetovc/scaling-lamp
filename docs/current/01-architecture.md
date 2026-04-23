# Архитектура

## Слои

Система состоит из трёх основных слоёв.

### 1. `agent-runtime`

[`crates/agent-runtime`](../../crates/agent-runtime) содержит доменную модель runtime:

- prompt assembly;
- provider contracts;
- tools и их schemas;
- sessions, runs, plans, missions, memory;
- inter-agent primitives;
- workspace abstraction;
- permissions.

Это слой “что такое runtime”.

### 2. `agent-persistence`

[`crates/agent-persistence`](../../crates/agent-persistence) отвечает за состояние:

- загрузка и валидация конфига;
- SQLite metadata store;
- transcripts/artifacts/runs payload storage;
- recovery policy;
- audit log;
- repository-style CRUD доступ.

Это слой “где и как лежат данные”.

### 3. `agentd`

[`cmd/agentd`](../../cmd/agentd) — интеграционный слой и операторский бинарь:

- bootstrap;
- CLI;
- daemon HTTP server/client;
- TUI;
- execution orchestration;
- MCP lifecycle;
- diagnostics;
- self-update/release info.

Это слой “как системой пользуются и как она связывает runtime с persistence”.

## Один канонический runtime path

В репозитории есть явный архитектурный запрет: не создавать второй chat path, второй prompt path или отдельный tool loop для “особого” интерфейса.

На практике это означает:

- Prompt собирается один раз по одной схеме.
- Provider loop живёт в одном месте: [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs).
- Tools исполняются через единый `ToolRuntime`/`ExecutionService`.
- TUI не делает “свои” скрытые execution shortcuts, а работает через backend над тем же приложением.

## Главные точки входа

### Bootstrap

[`cmd/agentd/src/main.rs`](../../cmd/agentd/src/main.rs) делает только одно:

- вызывает `bootstrap::build()`;
- получает `App`;
- запускает `App::run()`.

Это важно: почти всё знание о wiring находится не в `main`, а в [`cmd/agentd/src/bootstrap.rs`](../../cmd/agentd/src/bootstrap.rs).

### App

`App` в `bootstrap.rs` связывает:

- `AppConfig`
- `PersistenceScaffold`
- `RuntimeScaffold`
- `SharedProcessRegistry`
- `SharedMcpRegistry`
- `RuntimeReleaseUpdater`

Через `App` доступны operator-facing методы вроде:

- `runtime_status_snapshot`
- `store`
- `provider_driver`
- session/agent/schedule/mcp rendering helpers

### ExecutionService

`ExecutionService` в [`cmd/agentd/src/execution.rs`](../../cmd/agentd/src/execution.rs) — главный orchestration слой. Он:

- загружает session/profile/run/job state;
- строит tool runtime;
- исполняет chat turns и background jobs;
- управляет provider loop;
- управляет inter-agent flow;
- управляет wakeups, schedules, delegate jobs и approvals.

Если вы меняете semantics выполнения — почти наверняка вы меняете `ExecutionService` или его подмодули.

## Подмодули execution

Подпапка [`cmd/agentd/src/execution`](../../cmd/agentd/src/execution) разбита по ответственности:

- `chat.rs` — chat-turn related entrypoints.
- `provider_loop.rs` — основной model/tool loop.
- `tools.rs` — bridge между runtime tool definitions и actual execution.
- `interagent.rs` — `message_agent`, `session_wait`, grants и child sessions.
- `background.rs` — background worker tick и job pumping.
- `mission.rs` — mission-turn logic.
- `memory.rs` — `session_read`, `session_search`, `knowledge_*`.
- `delegation.rs` / `delegate_jobs.rs` / `wakeup.rs` / `supervisor.rs` — сервисная orchestration-логика.

Именно это разделение помогает держать систему понятной для начинающего разработчика: один файл — одна группа сценариев.

## Surface layers

### CLI

[`cmd/agentd/src/cli.rs`](../../cmd/agentd/src/cli.rs) парсит команды и отправляет их:

- либо напрямую в `App`;
- либо через daemon client, если команда поддерживается по HTTP.

### HTTP server/client

- Сервер: [`cmd/agentd/src/http/server.rs`](../../cmd/agentd/src/http/server.rs)
- Клиент: [`cmd/agentd/src/http/client.rs`](../../cmd/agentd/src/http/client.rs)

Клиент умеет:

- подключаться к уже запущенному daemon;
- перезапускать несовместимый локальный daemon;
- autospawn’ить локальный daemon при необходимости;
- проверять build compatibility по `version/commit/tree_state/build_id`.

### TUI

- Entry: [`cmd/agentd/src/tui.rs`](../../cmd/agentd/src/tui.rs)
- State/UI model: [`cmd/agentd/src/tui/app.rs`](../../cmd/agentd/src/tui/app.rs)
- Backend trait: [`cmd/agentd/src/tui/backend.rs`](../../cmd/agentd/src/tui/backend.rs)

TUI intentionally thin: он хранит UI state, но не имеет собственного execution engine.

## Хранилище

Главный объект persistence — [`PersistenceStore`](../../crates/agent-persistence/src/store.rs).

Он знает layout:

- `state.sqlite`
- `artifacts/`
- `archives/`
- `runs/`
- `transcripts/`

И умеет открываться в двух режимах:

- `open()` — bootstrap + reconcile path;
- `open_runtime()` — лёгкий request path без тяжёлой bootstrap work.

Это различие критично для производительности TUI/HTTP.

## Архитектурный data flow

Упрощённо:

1. Surface получает команду.
2. Surface вызывает `App`.
3. `App` открывает runtime store и/или `ExecutionService`.
4. `ExecutionService` читает состояние через repositories `PersistenceStore`.
5. Runtime layer строит prompt, tools и provider request.
6. Execution result сохраняется в `PersistenceStore`.
7. Surface читает обновлённую session summary/transcript/jobs/approvals.

## Что важно не ломать

- Канонический prompt assembly order.
- Один provider loop.
- Асинхронный, но наблюдаемый inter-agent flow.
- Разделение `open()` и `open_runtime()`.
- Тонкость CLI/TUI/HTTP слоёв.
- Structured tools как основной capability surface.
