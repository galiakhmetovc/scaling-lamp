# Operator Chat

## Зачем он нужен

`teamd-agent chat <chat_id> <session_id>` — это не отдельный runtime path и не "мини-Telegram" в терминале.

Это operator console поверх уже существующего control plane:

- `POST /api/runs`
- `GET /api/events/stream`
- approvals API
- plans API
- workers/jobs APIs
- artifacts API
- control-state API

Поэтому chat не дублирует orchestration. Он только:

- отправляет user input
- читает live events
- даёт inline operator actions
- печатает readable timeline

## Что реально видно в чате

В интерактивном режиме chat рендерит:

- `you:` твоя реплика
- `assistant:` финальный ответ
- `system:` run lifecycle и fallback status
- `approval:` approval requests и decisions
- `worker:` worker lifecycle, waiting approval, handoff
- `job:` background jobs
- `plan:` persisted plan events
- `memory:` artifact offload и related runtime signals

Это сознательно не raw log stream.  
Chat показывает только наблюдаемое operator-facing состояние.

## Какие команды есть внутри

- `/help`
- `/status`
- `/approve <approval_id>`
- `/reject <approval_id>`
- `/plan`
- `/plans`
- `/handoff <worker_id>`
- `/artifact <ref>`
- `/cancel`
- `/quit`

## Approval ids

Chat знает уже увиденные approval ids и помогает с ними работать.

Сейчас поддержано:

- `tab` completion для известных approval ids
- unique-prefix resolution

То есть если консоль уже показала:

```text
approval: requested approval-1775984578647293815 for shell.exec
```

то команда

```text
/approve approval-17759
```

сработает, если это уникальный префикс известного approval id.

Если id неправильный или неоднозначный, chat пишет readable `system:` ошибку и не падает.

## Worker approvals

Если approval нужен не главному run, а worker'у, chat тоже должен это показать.

Сейчас оператор видит:

- `worker: worker-3 approval requested approval-... for shell.exec`
- `/status` показывает worker со статусом `waiting_approval` и связанным `approval=<id>`

Это важно, потому что иначе parent run выглядит просто как "висит", а реальная причина скрыта в дочернем worker run.

## Как chat связан с control plane

Chat использует тот же runtime state, что и остальные поверхности:

- persisted runs
- persisted approvals
- persisted workers/jobs/plans
- persisted runtime events
- persisted final responses
- persisted prompt budget snapshots

Поэтому поведение в chat должно совпадать с:

- `teamd-agent runs ...`
- `teamd-agent events ...`
- `teamd-agent approvals ...`
- `teamd-agent workers ...`

Если между ними есть расхождение, это баг в control plane или в chat renderer.

## Context and budget

Теперь operator surfaces должны различать как минимум два числа:

- full context window percent
- prompt budget percent

Если кажется, что контекст "теряется слишком рано", смотреть нужно прежде всего на `prompt budget`, а не только на общий процент окна.

Подробности смотри в [context-budget.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/context-budget.md).

## Governance baseline

Chat наследует тот же execution policy baseline, что и live runtime:

- disallowed MCP tools не должны вообще экспонироваться как доступные tool actions
- approval-required tools идут в approval path, а не исполняются молча
- длинный tool output режется policy limits ещё до того, как попадёт в readable timeline

## Где смотреть код

- [cmd/coordinator/cli.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/cli.go)
- [internal/cli/chat_console.go](/home/admin/AI-AGENT/data/projects/teamD/internal/cli/chat_console.go)
- [internal/cli/chat_console_test.go](/home/admin/AI-AGENT/data/projects/teamD/internal/cli/chat_console_test.go)
- [internal/cli/client.go](/home/admin/AI-AGENT/data/projects/teamD/internal/cli/client.go)
- [internal/api/server.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/server.go)

## Что важно понимать

Operator chat нужен не как "удобный shell", а как контрольная поверхность для длинных задач:

- видеть live progress
- давать approvals
- наблюдать workers/jobs
- читать artifacts
- понимать, как compaction/memory/tool loop отражаются снаружи

Это bridge между runtime и будущим UI, но уже без Telegram-зависимости.

## Chat vs Web Test Bench

Теперь у runtime есть ещё одна локальная поверхность:

- operator chat
- web session test bench

Они не конкурируют за orchestration. Оба работают поверх одного control plane.

Разница такая:

- chat
  - удобнее для operator actions и длинной интерактивной эксплуатации
- web test bench
  - удобнее для тестирования того, как живёт сама session:
    - transcript timeline
    - compaction
    - pruning
    - SessionHead
    - recent-work binding
    - recall provenance
    - artifact flow

Если цель — понять, почему агент "забыл" контекст или почему compaction сработал рано, web test bench полезнее chat.
