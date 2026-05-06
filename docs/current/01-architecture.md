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
- PostgreSQL metadata/control-plane store;
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
- `mem0.rs` — optional semantic long-term memory tools `memory_*` поверх Mem0/OpenMemory REST API.
- `kv.rs` — deterministic scoped runtime KV tools `kv_*` поверх PostgreSQL table `kv_entries`.
- `scopes.rs` — общий mapping scopes `operator`, `agent`, `agent_shared`, `workspace`, `session` для Mem0 и KV.
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

- PostgreSQL metadata/control-plane store
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

## Граница web extraction

`web_search` и `web_fetch` остаются частью того же канонического tool surface. Для них не создаётся отдельный prompt path, отдельный daemon или special-case loop.

- `web_search` отвечает только за поиск результатов через настроенный backend (`duckduckgo_html` или `searxng_json`).
- `web_fetch` делает прямой HTTP fetch указанного URL.
- Если ответ HTML, runtime конвертирует его в markdown-подобный readable text через `html-to-markdown-rs` внутри [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs), а не прокидывает сырой HTML в модель.
- Если результат слишком большой, в prompt попадает только compact preview + offload ref, а полный payload сохраняется в artifact.

Это важная архитектурная граница: `html-to-markdown-rs` — внутренняя библиотека текущего runtime path. Более тяжёлый document-ingestion слой вроде Kreuzberg допустим только как внешний optional addon для файлов/вложений, но не как замена каноническому `web_fetch`.

Если нужен настоящий JS-capable browser, используется built-in `browser_*` surface. Runtime вызывает `agent-browser` CLI, а production backend обычно Browserless. Это всё равно тот же `automatic_provider_tools(...)`, общий provider loop, tool ledger, artifacts/offload и debug UI. Legacy browser MCP connectors, например Lightpanda, допустимы только как optional/experimental extension и не создают второй loop.

## Граница semantic memory

`memory_add`, `memory_search`, `memory_list`, `memory_update` и `memory_delete` являются built-in structured tools, но появляются в model-facing списке только когда включён `[mem0].enabled = true`.

Mem0/OpenMemory в этой схеме — внешний durable semantic index, а не runtime store:

- PostgreSQL, transcripts, runs, tool ledger, schedules и artifacts остаются в `agent-persistence`;
- prompt не получает “скрытую память”: optional `memory_recall` делает bounded pre-turn search и вставляет результат отдельным видимым блоком `Memory Recall`; для дополнительных деталей агент должен явно вызвать `memory_search`/`memory_list`;
- запись в память происходит либо явным `memory_add`/`memory_update`, либо optional post-turn `memory_curator`, который запускается после завершения chat turn и применяет candidates через тот же Mem0 слой; prompt curator лежит в `data_dir/agent-templates/system/memory-curator/SYSTEM.md` и редактируется без пересборки;
- tool calls проходят через тот же provider loop, approvals, ledger и debug UI;
- Mem0 REST endpoint настраивается в config/env, но не создаёт второй chat path;
- Mem0 scopes мапятся на Mem0 entities: `operator -> user_id`, `agent -> agent_id`, `agent_shared -> agent_id=teamd-agent-shared`, `workspace -> agent_id=teamd-workspace-<hash>`, `session -> run_id`;
- общий пул памяти агентов реализован как `agent_shared`, но это semantic pool, а не deterministic KV.

Такое разделение оставляет semantic memory inspectable и отключаемой: если Mem0, recall или curator недоступны, основной chat turn не должен падать.

## Граница deterministic KV

`kv_get`, `kv_put`, `kv_list` и `kv_delete` являются built-in structured tools и живут в PostgreSQL table `kv_entries`, а не в Mem0, MCP или внешнем Redis.

KV нужен для точного состояния вида `key -> JSON value`:

- настройки и флаги, которые агент должен прочитать по точному ключу;
- counters, cursors, lightweight locks и markers;
- durable scratch state, который не должен искаться семантически;
- small coordination state между sessions/agents/workspaces.

Scopes у KV такие же логически, как у Mem0, но мапятся не на Mem0 entities, а на `(scope, namespace_id, key)`:

- `operator -> namespace_id = mem0.default_user_id`;
- `agent -> namespace_id = <agent_profile_id>`;
- `agent_shared -> namespace_id = teamd-agent-shared`;
- `workspace -> namespace_id = teamd-workspace-<sha256(workspace_root)[0..16]>`;
- `session -> namespace_id = <session_id>`.

Инфраструктурное решение намеренно простое: PostgreSQL уже является canonical durable store, попадает в backup/recovery flow и работает через те же retry/transaction правила. Redis/etcd для KV пока не нужны: они добавили бы второй operational plane без выигрыша для текущего runtime.
