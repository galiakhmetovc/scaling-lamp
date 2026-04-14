# Approvals

Approvals нужны для risky действий, которые runtime не должен исполнять молча.

Сейчас это как минимум:

- `shell.exec`
- `filesystem.write_file`

## Как это работает

1. Модель просит risky tool.
2. Transport/tool runtime не исполняет его сразу.
3. Создаётся approval record.
   В нём сразу фиксируются audit-поля:
   - `reason`
   - `target_type`
   - `target_id`
   - `requested_at`
4. Пользователь видит pending approval.
5. Approval можно принять или отклонить:
   - из Telegram callback
   - через HTTP API
   - через CLI
6. Decision обновляет audit trail:
   - `status`
   - `decided_at`
   - `decision_update_id`
7. Если approval принят, тот же run продолжает выполнение дальше.

## Где хранится состояние

Runtime stores:

- `runtime_approvals`
- `runtime_approval_callbacks`
- `runtime_approval_continuations`

Это значит:

- approval state переживает restart
- callback idempotency тоже переживает restart
- pending guarded continuation тоже переживает restart
- approval audit metadata тоже переживает restart

Run state при ожидании approval помечается как `waiting_approval` в runtime store.

## Policy snapshot

Approvals теперь живут рядом с persisted policy snapshot того execution context, который их породил.

Это значит:

- каждый `run` хранит effective `runtime + memory + action` policy
- каждый `job` хранит свой policy snapshot
- каждый `worker` хранит свой policy snapshot

Практический смысл:

- можно понять, почему approval вообще понадобился
- session overrides не переписывают старые execution records задним числом
- audit trail читается вместе с тем policy state, в котором жил конкретный run/job/worker

## Resume после approve

Есть два path:

1. Живой waiter в том же процессе.
2. Restart-safe callback resume.

Если процесс всё ещё жив, approval будит тот же ожидающий run.

Если процесса уже нет, callback:

1. читает continuation record,
2. исполняет одобренный tool,
3. дописывает `tool` message в transcript,
4. поднимает continuation run через runtime loop,
5. удаляет continuation record после завершения.

Это важно: resume идёт не через повторный user prompt, а через сохранённый tool continuation path.

## Где смотреть код

- [internal/approvals/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/approvals/service.go)
- [internal/runtime/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [internal/runtime/sqlite_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/sqlite_store.go)
- [internal/runtime/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)
- [internal/transport/telegram/provider_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/provider_tools.go)
- [internal/api/server.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/server.go)

## Что пока ещё не идеально

- approval FSM всё ещё относительно простая
- richer policy routing по side-effect class ещё можно усиливать
- restart-safe continuation уже есть, но platform-grade recovery ещё можно расширять метаданными и replay tooling
