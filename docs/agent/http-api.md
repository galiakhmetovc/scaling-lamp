# HTTP API

`teamd-agent` теперь поднимает HTTP API в том же бинарнике, что и Telegram runtime.

По умолчанию API слушает локальный адрес из `TEAMD_API_LISTEN_ADDR`.

Если задан `TEAMD_API_AUTH_TOKEN`, почти все ` /api/* ` endpoints требуют:

```http
Authorization: Bearer <token>
```

Исключение одно: `GET /api/runtime` оставлен без токена для локального smoke/health-check.

## Зачем он нужен

Это главный control surface для single-agent runtime.

Через него работают:

- CLI
- Telegram approval/status integration
- local web session test bench
- jobs control plane
- workers control plane
- plans control plane

Идея простая:

- runtime живёт один раз
- клиенты только вызывают его через стабильный API

Effective execution governance тоже живёт в runtime:

- MCP allowlist
- approval-required tools
- execution time/output limits

То есть API и transports должны видеть один и тот же policy answer, а не собирать его локально по кускам.

## Основные endpoints

### Runtime

- `GET /api/runtime`
  - глобальный runtime summary
- `GET /api/runtime?session_id=<id>`
  - effective runtime summary для сессии
- `GET /api/runtime/sessions/{session_id}`
  - compatibility alias для session runtime summary
- `PATCH /api/runtime/sessions/{session_id}`
  - compatibility alias для сохранения session-scoped overrides
- `DELETE /api/runtime/sessions/{session_id}`
  - compatibility alias для очистки session-scoped overrides

### Sessions

- `GET /api/sessions?chat_id=<id>&limit=<n>`
  - список runtime-known сессий
- `GET /api/sessions/{session_id}`
  - session-centric view:
    - effective runtime summary
    - latest run
    - pending approvals count
- `PATCH /api/sessions/{session_id}`
  - сохранить session-scoped overrides
- `DELETE /api/sessions/{session_id}`
  - очистить session-scoped overrides
- `POST /api/session-actions`
  - generic chat-scoped session management actions
  - текущие действия:
    - `session.show`
    - `session.create`
    - `session.use`
    - `session.list`
    - `session.stats`
    - `session.reset`

### Debug Web Test Bench

Локальный web test bench живёт в том же бинарнике:

- `GET /debug/test-bench`
  - embedded shell для локального тестирования сессий
- `GET /debug/assets/app.js`
- `GET /debug/assets/styles.css`

Это не отдельный runtime path.  
Web shell использует те же API endpoints, что и другие operator surfaces.

Для интерактивной части phase 1 используются:

- `GET /api/sessions?chat_id=<id>`
  - session picker
- `POST /api/session-actions`
  - создать новую session через `session.create`
- `POST /api/debug/sessions/{session_id}/messages`
  - отправить новое user message в выбранную session
- `GET /api/debug/sessions/{session_id}?chat_id=<id>&event_limit=<n>`
  - snapshot selected session + control state + timeline events
- `GET /api/debug/runs/{run_id}`
  - run snapshot и replay
- `GET /api/debug/runs/{run_id}/context-provenance`
  - provenance view:
    - `SessionHead`
    - `recent_work`
    - transcript
    - memory recall
    - checkpoint
    - continuity
    - workspace
    - skills

Это делает web UI пригодным не просто для общения, а именно для тестирования:

- compaction
- pruning
- SessionHead
- recent-work binding
- artifact offload
- prompt layer provenance

### Events

- `GET /api/events?entity_type=<type>&entity_id=<id>&run_id=<id>&session_id=<id>&after_id=<n>&limit=<n>`
  - cursor-based read of persisted runtime events
  - работает для `run`, `job`, `worker`
  - это polling-friendly surface поверх того же persisted event plane
- `GET /api/events/stream?entity_type=<type>&entity_id=<id>&run_id=<id>&session_id=<id>&after_id=<n>&limit=<n>`
  - `text/event-stream` surface поверх того же `runtime_events`
  - stream отдаёт `event: runtime` и JSON `RuntimeEvent` в `data:`
  - `after_id` позволяет клиенту продолжить чтение с известного cursor
  - если tool output был вынесен в artifact store, runtime эмитит `artifact.offloaded`
  - payload такого события несёт `artifact_ref`, `tool_name`, `tool_call_id`
  - persisted plans пишут:
    - `plan.created`
    - `plan.updated`
    - `plan.item_started`
    - `plan.item_completed`

### Control

- `GET /api/control/{session_id}?chat_id=<id>`
  - generic session-scoped control snapshot:
    - latest run
    - pending approvals
    - active workers
    - active jobs
- `POST /api/control/{session_id}/actions`
  - выполнить generic control action
  - текущие действия:
    - `run.status`
    - `run.cancel`

### Debug

- `GET /api/debug/sessions/{session_id}?chat_id=<id>&event_limit=<n>`
  - debug session view:
    - `session`
    - `control`
    - persisted runtime `events`
- `POST /api/debug/sessions/{session_id}/messages`
  - bounded write path для local web test bench
  - стартует новый run в уже выбранной session
- `GET /api/debug/runs/{run_id}?event_limit=<n>`
  - debug run view:
    - `run`
    - `replay`
    - persisted runtime `events`
- `GET /api/debug/runs/{run_id}/context-provenance`
  - runtime-owned provenance snapshot для инспекции assembled context

### Plans

- `GET /api/plans?owner_type=<type>&owner_id=<id>&limit=<n>`
  - список persisted plans для `run` или `worker`
- `GET /api/plans/{id}`
  - полный plan record с notes и items
- `POST /api/plans`
  - создать plan
- `PUT /api/plans/{id}/items`
  - заменить items целиком
- `POST /api/plans/{id}/notes`
  - append note
- `POST /api/plans/{id}/items/{item_id}/start`
  - пометить item как `in_progress`
- `POST /api/plans/{id}/items/{item_id}/complete`
  - пометить item как `completed`

### Runs

- `GET /api/runs?chat_id=<id>&session_id=<id>&status=<status>&limit=<n>`
  - список run-ов с фильтрами
- `POST /api/runs`
  - создать run
- `GET /api/runs/{id}`
  - получить status, persisted `policy_snapshot` и `artifact_refs`
- `GET /api/runs/{id}/replay`
  - получить operator-facing replay timeline из persisted run и runtime events
- `POST /api/runs/{id}/cancel`
  - отменить run

### Approvals

- `GET /api/approvals?session_id=<id>`
  - список pending approvals по сессии
  - каждая запись несёт:
    - `reason`
    - `target_type`
    - `target_id`
    - `requested_at`
- `POST /api/approvals/{id}/approve`
  - возвращает updated approval record с `decided_at` и `decision_update_id`
- `POST /api/approvals/{id}/reject`
  - возвращает updated approval record с `decided_at` и `decision_update_id`

### Memory

- `GET /api/memory/search?chat_id=<id>&session_id=<id>&query=<text>&limit=<n>`
  - semantic/text recall over stored memory documents
- `GET /api/memory/{doc_key}`
  - full memory document by key

### Jobs

- `GET /api/jobs?limit=<n>`
  - список background jobs
- `POST /api/jobs`
  - создать detached job
  - job получает persisted `policy_snapshot`
- `GET /api/jobs/{id}`
  - получить status job и persisted `policy_snapshot`
- `GET /api/jobs/{id}/logs`
  - получить stdout/stderr chunks
- `POST /api/jobs/{id}/cancel`
  - запросить cancel

### Workers

- `GET /api/workers?chat_id=<id>`
  - список managed workers
- `POST /api/workers`
  - spawn worker
  - worker получает persisted `policy_snapshot`
- `GET /api/workers/{id}`
  - worker state, persisted `policy_snapshot` и `artifact_refs` последнего worker run
- `POST /api/workers/{id}/messages`
  - отправить worker новый input
- `GET /api/workers/{id}/wait?after_cursor=<n>&after_event_id=<n>`
  - получить новые worker messages, events и текущий handoff без блокировки
- `GET /api/workers/{id}/handoff`
  - canonical parent-facing worker handoff
- `POST /api/workers/{id}/close`
  - закрыть worker

## Как думать про API

У API теперь три уровня control plane:

- `runs`
  - обычный агентный lifecycle пользователя
- `jobs`
  - background execution primitive
- `workers`
  - local managed subagents

И два сквозных слоя:

- `policy_snapshot`
  - фиксирует effective runtime/memory/action policy у run/job/worker
- `approval audit`
  - фиксирует, почему risky действие потребовало decision и чем decision закончился
- `artifact offload`
  - большие tool outputs не обязаны жить inline в transcript
  - run view даёт `artifact_refs`
  - event plane даёт `artifact.offloaded`
  - полный payload читается через `/api/artifacts/{ref}` и `/api/artifacts/{ref}/content`
- `plans`
  - держат активный skeleton работы как persisted runtime state
  - не заменяют continuity и memory, а дополняют их
- `worker handoff`
  - даёт структурированный результат worker execution для parent path
  - уменьшает зависимость от raw worker transcript

### Artifacts

- `GET /api/artifacts/{ref}`
  - metadata по сохранённому артефакту
- `GET /api/artifacts/{ref}/content`
  - полное содержимое артефакта
- `GET /api/artifacts/search?owner_type=<type>&owner_id=<id>&query=<text>&limit=<n>`
  - scoped-first search over offloaded artifacts
  - основной путь: искать в пределах `run` / `worker`
  - global search разрешён только явно через `global=true`

## Где смотреть код

- [server.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/server.go)
- [types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/types.go)
- [errors.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/errors.go)
- [runtime_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/runtime_api.go)

## Как тестировать

Локально:

```bash
curl http://127.0.0.1:18081/api/runtime
curl http://127.0.0.1:18081/api/sessions?chat_id=1001
curl http://127.0.0.1:18081/api/sessions/1001:default
curl http://127.0.0.1:18081/api/events?entity_type=run\&entity_id=run-1
curl -N http://127.0.0.1:18081/api/events/stream?entity_type=run\&entity_id=run-1
curl http://127.0.0.1:18081/api/plans?owner_type=run\&owner_id=run-1
curl http://127.0.0.1:18081/api/plans/plan-1
curl http://127.0.0.1:18081/api/runs?session_id=1001:default
curl http://127.0.0.1:18081/api/artifacts/artifact:%2F%2Ftool-output-1
curl http://127.0.0.1:18081/api/artifacts/artifact:%2F%2Ftool-output-1/content
curl http://127.0.0.1:18081/api/jobs
curl http://127.0.0.1:18081/api/workers?chat_id=1001
curl -X PATCH http://127.0.0.1:18081/api/sessions/1001:default \
  -H 'Content-Type: application/json' \
  -d '{"runtime":{"model":"glm-5.1"},"memory_policy":{"profile":"standard"}}'
```
