# Runtime API Walkthrough

## Главная идея

`internal/runtime/runtime_api.go` — это transport-agnostic query/lifecycle surface над runtime.

Но после `AgentCore` он уже не единственный верхний runtime layer.

Сейчас правильная иерархия такая:

- `runtime.API`
  - store-backed runtime primitives
- `AgentCore`
  - canonical orchestration facade для API и transports

Он должен отвечать на вопросы уровня:

- создать или отменить run
- посмотреть run status
- посмотреть control state сессии
- исполнить generic control actions
- исполнить generic session actions
- увидеть approvals
- принять approval decision
- прочитать или сохранить session overrides
- посмотреть jobs, workers, plans, artifacts, events
- построить debug session/run/provenance views для local web test bench

А не на вопросы Telegram UI.

## Что делает runtime.API

### Run lifecycle

- использует [run_manager.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/run_manager.go)
- работает поверх `ActiveRegistry`
- пишет run state в persistent store

### Approval lifecycle

- использует [internal/approvals/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/approvals/service.go)
- умеет:
  - list pending approvals
  - approve/reject
  - переживать restart через store

### Control surface primitives

- использует [control_actions.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/control_actions.go)
- использует [session_actions.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/session_actions.go)
- умеет строить:
  - `ControlState`
  - `FormatControlReport(...)`
  - generic `run.status` / `run.cancel`
  - generic `session.show/create/use/list/stats/reset`

### Event-backed runtime views

Facade теперь опирается не только на run records, но и на persisted event plane.

Это нужно, чтобы:

- surfaced `artifact.offloaded`
- surfaced `worker.approval_requested`
- читать `FinalResponse`
- поддерживать CLI `events watch` и operator chat
- поддерживать local web session test bench:
  - transcript timeline
  - prompt budget snapshots
  - SessionHead visibility
  - run-level context provenance

### Debug views

- использует [debug_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/debug_service.go)
- использует [debug_views.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/debug_views.go)
- строит:
  - `DebugSessionView`
  - `DebugRunView`
  - `DebugContextProvenance`

Это нужно, чтобы web test bench не склеивал runtime truth сам из случайных API вызовов.

### Session overrides

- читает и пишет `runtime_session_overrides`
- строит effective summary:
  - runtime request config
  - memory policy
  - action policy

### Governance baseline

- использует [policy_resolver.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/policy_resolver.go)
- собирает:
  - effective session policy
  - MCP execution policy
  - tool execution decisions

Это нужно, чтобы один и тот же ответ на вопросы:

- разрешён ли local MCP tool
- нужен ли approval
- какие time/output limits применяются

использовался одинаково и в API, и в transport paths.

## Где граница

Если код зависит от:

- Telegram callback data
- Telegram chat message ids
- status card UX

то ему не место в runtime API.

Если код должен одинаково работать из:

- HTTP API
- CLI
- operator chat
- Telegram

то ему место либо здесь, либо в runtime-owned service рядом, либо в [agent_core.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/agent_core.go), если это orchestration facade level.

Смотри также:

- [agentcore.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/agentcore.md)
- [http-api.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/http-api.md)
- [cli.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/cli.md)
- [operator-chat.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/operator-chat.md)
