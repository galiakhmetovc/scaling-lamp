# Core Architecture Walkthrough

Этот документ объясняет бота как учебное single-agent ядро.

## 1. Вход

Внешний мир приходит не только через Telegram.

Сейчас есть три поверхности:

- runtime core
- HTTP API
- transports/clients

Практически:

- Telegram — transport
- CLI — API client, включая `events watch` и `chat`
- будущий Web UI — тоже API client

Файлы:

- [adapter.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/adapter.go)
- [immediate_updates.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/immediate_updates.go)
- [run_lifecycle.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/run_lifecycle.go)

Что происходит:

- `Poll(...)` читает updates
- `Dispatch(...)` решает, это:
  - slash-команда
  - callback
  - обычный user run
- для обычного запроса создаётся `run`, пишется `user` message и поднимается status card

Идея:

- transport не должен “думать за агента”
- transport только принимает input, запускает run и показывает output

## 2. Run

Run — это одна обработка одного пользовательского запроса.

Файлы:

- [active_registry.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/active_registry.go)
- [run_manager.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/run_manager.go)
- [run_state.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/run_state.go)

Важно различать:

- `ActiveRegistry`
  - кто реально выполняется сейчас
  - кого можно отменить через context cancel
- Telegram `RunState`
  - только UI состояние карточки статуса
- runtime store
  - что сохраняется про run в БД

Идея:

- execution state, UI state и persistence state не должны быть одной сущностью

## 2.5 HTTP API

Файлы:

- [internal/api/server.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/server.go)
- [internal/api/types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/types.go)
- [internal/runtime/runtime_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/runtime_api.go)

Идея:

- HTTP API — не отдельный runtime
- это просто стабильная внешняя поверхность над тем же runtime core
- а canonical orchestration center для него теперь явно собран в [agentcore.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/agentcore.md)

Через него идут:

- run create/status/cancel
- run list/filter
- operator chat console поверх того же API
- approvals
- session list/show
- session overrides
- plans
- jobs
- workers

## 2.6 Runtime Execution Service

Файлы:

- [execution_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/execution_service.go)
- [runtime_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/runtime_api.go)

Идея:

- `runtime.API` хранит lifecycle primitives
- `ExecutionService` владеет orchestration для:
  - start run
  - detached start for HTTP API
  - approval continuation resume

Transport не вызывает `PrepareRun/LaunchRun` напрямую.
Он даёт runtime hooks:

- что сделать перед стартом
- как выполнить run
- как выполнить approval resume

Это и есть transport-agnostic boundary:

- runtime владеет orchestration
- transport владеет input/output UX

## 2.7 AgentCore

Файлы:

- [agent_core.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/agent_core.go)
- [agentcore.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/agentcore.md)

Сейчас правильная mental model такая:

- `runtime.API` — store-backed runtime queries и lifecycle primitives
- `ExecutionService` — run orchestration
- `JobsService` / `WorkersService` / `SessionActions` — узкие service slices
- `AgentCore` — canonical facade, через который transport и HTTP API должны видеть runtime

Это не god object и не замена narrow services.

Это явный runtime-owned центр тяжести.

## 3. Prompt Assembly

Это место, где агент получает контекст.

Файлы:

- [conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
- [prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/prompt_context.go)

Путь такой:

1. загрузить raw session history
2. projected prompt budget check
3. если нужно, сделать compaction
4. сделать pruning старого prompt residency
5. взять checkpoint
6. собрать base prompt через `compaction.AssemblePrompt(...)`
7. добавить runtime-owned fragments:
   - workspace context
   - SessionHead
   - memory recall
   - skills catalog
   - active skills

Идея:

- core prompt assembly живёт в runtime
- Telegram только отображает и передаёт runtime-owned budget/context metrics

## 4. Tool Loop

Файлы:

- [conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [provider_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/provider_tools.go)
- [memory_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_tools.go)
- [runtime_guards.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/runtime_guards.go)

Путь:

1. runtime вызывает `provider.Generate(...)`
2. модель возвращает:
   - либо текст
   - либо tool calls
3. если пришли tool calls:
   - runtime пишет `assistant` message с tool calls
   - вызывает tool executor
   - пишет `tool` result
   - идёт в следующий round

Guardrails:

- provider round timeout
- advisory stop
- repeated identical tool-call breaker
- `/cancel`
- artifact offload for large tool outputs

Идея:

- модель не исполняет tools сама
- runtime исполняет tools и контролирует цикл
- большие tool outputs не обязаны жить inline в transcript

## 5. Memory

Файлы:

- [memory_runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_runtime.go)
- [memory_documents.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/memory_documents.go)
- [recall.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/recall.go)
- [postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/postgres_store.go)

Понимать это лучше так:

- `session history`
- `working state`
- `searchable memory`

Где:

- `checkpoint` и `continuity` — это `working state`
- `memory documents` — это `searchable memory`

Идея:

- не вся история становится памятью
- в searchable memory попадает только то, что прошло policy/promotion rules

## 6. Storage

### Runtime storage

Файлы:

- [store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)

Роли:

- `RunLifecycleStore`
- `PlanStore`
- `JobStore`
- `WorkerStore`
- `SessionStateStore`
- `ProcessedUpdateStore`
- `SessionOverrideStore`
- `ApprovalStateStore`

### Telegram session storage

Файлы:

- [store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/store.go)
- [session_transcript_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_transcript_store.go)
- [session_checkpoint_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_checkpoint_store.go)
- [session_selector_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_selector_store.go)
- [postgres_transcript_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/postgres_transcript_store.go)
- [postgres_checkpoint_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/postgres_checkpoint_store.go)

Роли:

- `TranscriptStore`
- `CheckpointStore`
- `SessionSelector`

Идея:

- storage читается лучше, когда transcript, checkpoint и active session не слиты в один интерфейс

## 6.5 Jobs And Workers

Это следующий слой после обычного single-agent runtime.

`Job`:

- detached process
- logs
- cancel
- recovery

`Worker`:

- local supervised subagent
- own worker session
- own inbox/outbox
- own run lifecycle

Почему это важно:

- `job` отвечает за background execution
- `worker` отвечает за delegated local reasoning

Их нельзя сливать в одну сущность без потери ясности.

Файлы:

- [jobs_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/jobs_service.go)
- [workers_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/workers_service.go)
- [jobs.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/jobs.md)
- [workers.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/workers.md)

## 6.6 Delegation Tools

Теперь основной агент может использовать control plane не только через API/CLI, но и через tools.

Файлы:

- [provider_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/provider_tools.go)
- [delegation_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/delegation_tools.go)

Текущие tools:

- `job_start`
- `job_status`
- `job_cancel`
- `agent_spawn`
- `agent_message`
- `agent_wait`

Идея:

- runtime владеет jobs/workers
- tool layer только даёт модели к ним доступ
- это и есть подготовка к более взрослой delegation модели

## 7. Launcher

Файлы:

- [teamd-agentctl](/home/admin/AI-AGENT/data/projects/teamD/scripts/teamd-agentctl)
- user systemd units

Идея:

- бот не должен жить на случайных shell-процессах
- запуск, перезапуск, status и logs должны быть стандартными

## 8. Как читать проект новичку

Минимальный путь:

1. [01-overview.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/01-overview.md)
2. [core-architecture-walkthrough.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/core-architecture-walkthrough.md)
3. [request-lifecycle.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/request-lifecycle.md)
4. [03-sessions-runs-and-cancellation.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/03-sessions-runs-and-cancellation.md)
5. [04-tool-loop.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/04-tool-loop.md)
6. [05-memory-and-recall.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/05-memory-and-recall.md)
7. [06-compaction.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/06-compaction.md)
8. [http-api.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/http-api.md)
9. [cli.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/cli.md)

Если после этого всё ещё не ясно, уже тогда идти в:

- [code-map.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/code-map.md)
