# Telegram Runtime Target Architecture

## Goal

Собрать для `teamD` устойчивый single-agent Telegram runtime, взяв:
- runtime discipline из `Hermes Agent`
- execution architecture из `OpenClaw`

План фиксирует только целевую архитектуру и порядок внедрения.

## Design Sources

### Hermes: what to borrow

- interruptible provider calls
- hard iteration budget
- per-tool timeout
- repeated-tool loop breakers
- continuity in persistent storage

### OpenClaw: what to borrow

- headless run manager
- active run registry
- cancel queued vs abort running
- separate checkpoint store
- continuity/archive separate from raw chat history

## Non-Goals

Не делаем в этом плане:
- mesh orchestration
- multi-agent coordination
- scheduler/cron orchestration
- deep persona/identity modeling
- full LangGraph migration

## Target Modules

### 1. RunManager

Новый headless модуль:
- принимает `chat_id`, `session_id`, `input`
- создаёт run
- ведёт lifecycle:
  - `queued`
  - `running`
  - `completed`
  - `failed`
  - `cancelled`

Должен жить отдельно от Telegram poll loop.

Persistence:
- in-memory state недостаточен
- нужен store-backed status layer для:
  - `run_status`
  - `started_at`
  - `ended_at`
  - `failure_reason`
  - `cancel_requested`

Suggested files:
- `internal/runtime/run_manager.go`
- `internal/runtime/run_state.go`
- `internal/runtime/run_manager_test.go`

### 2. ActiveRunRegistry

Реестр активных run-ов по `chat_id` / `session_id`.

Функции:
- `StartRun`
- `ActiveRun`
- `CancelQueued`
- `AbortRunning`
- `FinishRun`

Требование:
- не больше одного обычного user run на chat
- `/status` и `/cancel` работают независимо

Важно:
- command path не должен стоять в той же очереди, что и обычные user runs
- `/cancel` должен обходить normal run queue

Suggested files:
- `internal/runtime/active_registry.go`
- `internal/runtime/active_registry_test.go`

### 3. Abortable Provider Layer

Обёртка над `provider.Generate`:
- per-round timeout
- explicit cancel support
- нормальная ошибка на timeout

Нужно явно различать:
- `logical cancel`
  - runtime перестал ждать ответ
- `physical cancel`
  - реальный HTTP request / stream действительно закрыт

Цель:
- добиться именно physical cancel, а не только logical stop

Требование:
- зависший round не висит бесконечно
- timeout и cancel отражаются в trace/status

Suggested files:
- `internal/provider/abortable.go`
- `internal/provider/abortable_test.go`

### 4. Tool Execution Guards

Отдельный слой guardrails:
- per-tool timeout
- repeated tool-call detection
- stop after repeated identical probes

Особенно для:
- `shell.exec`
- repeated `filesystem.read/list`
- repeated `skills.list/read`

Нужен recovery policy:
- не просто abort run
- а системный сигнал модели:
  - loop breaker triggered
  - choose alternative approach
  - or ask the user

Также нужен structured event:
- `guard_triggered`

Suggested files:
- `internal/runtime/tool_guard.go`
- `internal/runtime/tool_guard_test.go`

### 5. Checkpoint Store

Checkpointing вынести в отдельный storage layer.

Checkpoint должен хранить:
- `what_happened`
- `what_matters_now`
- `originating_intent`
- `updated_at`

Нужны trigger rules:
- after tool-heavy run
- on context pressure threshold
- before explicit compaction
- optionally by command

Требование:
- checkpoint не подменяет raw history
- checkpoint не живёт без user intent

Suggested files:
- `internal/runtime/checkpoints.go`
- `internal/runtime/checkpoints_test.go`

### 6. Canonical Session Continuity

Continuity отделить от transcript.

Минимально нужны 3 слоя:
- `raw session log`
- `checkpoint summary`
- `canonical continuity`

`canonical continuity` должен содержать:
- last stable user goal
- current task state
- latest resolved facts
- unresolved items

Форма должна быть строго типизированной, а не free-text blob.

Пример структуры:
- `user_goal`
- `current_state`
- `resolved_facts[]`
- `unresolved_items[]`
- `updated_at`

Suggested files:
- `internal/runtime/canonical_session.go`
- `internal/runtime/canonical_session_test.go`

### 7. Runtime Store

Нужен отдельный store interface для persistence:
- run records
- checkpoints
- canonical continuity

Для MVP допустим `sqlite`.

Suggested files:
- `internal/runtime/store.go`
- `internal/runtime/sqlite_store.go`
- `internal/runtime/sqlite_store_test.go`

## Telegram Integration

### Current Problem

Сейчас Telegram transport:
- сам poll-ит
- сам запускает run
- сам ждёт его завершения

Из-за этого:
- `/cancel` не работает во время stuck run
- input loop блокируется

### Target

Telegram adapter должен стать thin transport layer:
- poll/update intake
- routing commands
- enqueue normal runs in `RunManager`
- display status

Он не должен быть владельцем runtime loop.

Нужна idempotency-защита:
- Telegram retry/update duplication не должен запускать run дважды

## Required Behavior

### Cancellation

- `/cancel` всегда обрабатывается, даже если активный run завис
- cancel должен:
  - abort provider call
  - interrupt current tool when possible
  - сменить run status на `cancelled`

### Timeouts

- каждый `provider round` имеет timeout
- каждый long-running tool имеет timeout
- timeout surface-ится пользователю как явная ошибка, а не вечное ожидание

### Loop Control

- hard max iterations
- soft warning на budget pressure
- loop breaker на repeated identical tool calls

### Continuity

- follow-up вроде `Ну?` не должен ломаться из-за потери intent
- если intent утрачен, runtime должен явно просить clarification

## Implementation Order

### Sprint 1. Core Runtime
1. `RunManager`
2. `ActiveRunRegistry`
3. `Runtime Store`
4. async Telegram integration

### Sprint 2. Runtime Resilience
5. abortable provider layer
6. `/cancel` end-to-end
7. provider round timeout
8. tool guardrails

### Sprint 3. Memory & Continuity
9. checkpoint store
10. canonical continuity layer

## Acceptance Criteria

- один stuck run не блокирует Telegram intake
- `/cancel` реально останавливает stuck run
- provider hang превращается в timeout error
- repeated tool loops останавливаются predictably
- follow-up after compaction сохраняет user intent
- transport, runtime, continuity и checkpointing разделены по слоям
- graceful restart:
  - pending/running runs либо восстанавливаются, либо явно помечаются failed/cancelled
- Telegram duplicate delivery не дублирует run
- structured logs есть минимум для:
  - `run_created`
  - `run_aborted`
  - `guard_triggered`
  - `continuity_updated`
