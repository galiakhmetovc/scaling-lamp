# Plans

`Plans` в `teamD` — это persisted runtime state для длинной задачи.

Это не memory document и не continuity summary.

План нужен, когда у run или worker есть многошаговое намерение, которое должно переживать tool rounds и быть видимым через API/CLI.

## Что хранится

У плана есть:

- `plan_id`
- `owner_type`
  - `run` или `worker`
- `owner_id`
- `title`
- `notes`
- `items`

У item есть:

- `item_id`
- `content`
- `status`
  - `pending`
  - `in_progress`
  - `completed`
  - `cancelled`
- `position`

## Чем план отличается от памяти

- `continuity`
  - описывает текущее состояние разговора
- `memory`
  - хранит долговечные факты для recall
- `plan`
  - хранит активный скелет работы

То есть план отвечает не на вопрос:

- что агент помнит

а на вопрос:

- какие шаги он считает текущей программой работы

## Runtime contract

Минимальные операции:

- create plan
- replace items
- append note
- start item
- complete item

Все они идут через runtime-owned state, а не через произвольный текст модели.

## Agent-managed plans

Plans теперь могут жить не только в operator control plane, но и внутри normal tool loop.

Идея такая:

- оператор по-прежнему может создавать и редактировать планы через API/CLI
- агент и worker теперь тоже могут делать это через runtime-owned tool surface

Базовые tool actions:

- `plan_create`
- `plan_replace_items`
- `plan_annotate`
- `plan_item_start`
- `plan_item_complete`

Это не отдельный planning subsystem. Это thin tool-facing bridge к уже существующему persisted plan state.

## API surface

- `GET /api/plans?owner_type=<type>&owner_id=<id>&limit=<n>`
- `GET /api/plans/{id}`
- `POST /api/plans`
- `PUT /api/plans/{id}/items`
- `POST /api/plans/{id}/notes`
- `POST /api/plans/{id}/items/{item_id}/start`
- `POST /api/plans/{id}/items/{item_id}/complete`

## CLI surface

```bash
teamd-agent plans list run run-1
teamd-agent plans show plan-1
teamd-agent plans create run run-1 "Investigate rollout"
teamd-agent plans replace-items plan-1 '["Inspect runtime events","Verify CLI output"]'
teamd-agent plans note plan-1 "Focus on runtime-owned state."
teamd-agent plans start-item plan-1 plan-1-item-1
teamd-agent plans complete-item plan-1 plan-1-item-1
```

`replace-items` принимает:

- JSON-массив строк
- или JSON-массив `PlanItem`
- или `@path/to/file.json`

## Events

Планы пишут свои изменения в runtime event plane:

- `plan.created`
- `plan.updated`
- `plan.item_started`
- `plan.item_completed`

Поэтому оператор может видеть не только текущее состояние плана, но и историю его изменений.

## Где смотреть код

- [plans_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/plans_service.go)
- [types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/types.go)
- [store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [sqlite_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/sqlite_store.go)
- [postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)
