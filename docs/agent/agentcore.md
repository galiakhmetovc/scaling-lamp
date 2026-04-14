# AgentCore

`AgentCore` — это runtime-owned orchestration facade.

Файлы:

- [agent_core.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/agent_core.go)
- [server.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/server.go)
- [adapter.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/adapter.go)

## Зачем он нужен

До него центр системы был размазан по нескольким сервисам:

- `runtime.API`
- `ExecutionService`
- `JobsService`
- `WorkersService`
- `SessionActions`

Каждый сервис по отдельности был нормальный, но transport и API handler'ы должны были знать слишком много про shape runtime.

`AgentCore` делает одну вещь явной:

- transports и HTTP API ходят в один canonical runtime contract

## Что он владеет

- start run
- detached start
- approval continuation resume
- run views и run list
- control state и control actions
- session actions
- approvals
- events
- sessions и session overrides
- plans
- jobs
- workers

## Что он не владеет

- Telegram callback ids
- Telegram status cards
- terminal rendering
- Web UI rendering
- policy engine как отдельной подсистемой

То есть `AgentCore` отдаёт runtime domain data, а не UI.

## Где он используется

### HTTP API

`internal/api/server.go` теперь идёт в `AgentCore` first, а старые runtime-specific поля оставлены как compatibility fallback для тестов и постепенной миграции.

### Telegram

Telegram adapter теперь использует `AgentCore` для:

- run control actions
- session actions
- runtime summary / overrides
- approvals

Telegram остаётся transport layer:

- polling
- callback routing
- message rendering
- status-card UX

### CLI

CLI не зовёт `AgentCore` напрямую.  
Он по-прежнему клиент HTTP API.

Но теперь API ниже него уже построен вокруг canonical runtime facade.

## Mental model

Если нужен ответ на вопрос:

- "какой один runtime surface канонический?"

ответ теперь такой:

- сначала смотри [agent_core.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/agent_core.go)
- потом смотри конкретные narrow services, которые он композирует
