# Sessions, Runs, And Cancellation

## Session

Session — это логический диалог пользователя.

Примеры:

- `default`
- отдельная named session через `/session new ...`

Session влияет на:

- историю сообщений
- active skills
- memory recall scope
- runtime config overrides

Основной storage path:

- [internal/transport/telegram/session.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session.go)
- [internal/transport/telegram/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/postgres_store.go)
- [internal/transport/telegram/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/store.go)

Внутри Telegram session store теперь тоже есть явные роли:

- `TranscriptStore`
- `CheckpointStore`
- `SessionSelector`

## Run

Run — это обработка одного входящего пользовательского запроса.

Run имеет:

- `run_id`
- `chat_id`
- `session_id`
- `query`
- `status`
- `started_at`
- `ended_at`

Главный runtime path:

- [internal/runtime/run_manager.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/run_manager.go)
- [internal/runtime/active_registry.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/active_registry.go)
- [internal/runtime/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [internal/runtime/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)

## Почему есть и `ActiveRegistry`, и Telegram `RunState`

Это два разных слоя:

- `ActiveRegistry`
  - execution state
  - кто реально сейчас выполняется
  - кого можно отменить через context cancel
- runtime store / run records
  - persistence
  - что сохранить про run lifecycle в базе
  - внутри `internal/runtime/store.go` это теперь не один мешок, а три явные роли:
    - `RunLifecycleStore`
    - `SessionStateStore`
    - `ProcessedUpdateStore`
- Telegram `RunState`
  - UI состояние карточки статуса
  - stage, round index, waiting state, last tool
  - не содержит context cancel и не участвует в concurrency control

Код:

- [internal/transport/telegram/run_state.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/run_state.go)
- [internal/transport/telegram/status_card.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/status_card.go)

## Cancel

`/cancel` не должен ждать завершения текущего run.

Сейчас cancel работает так:

- transport получает `/cancel`
- Telegram UI state помечается как `cancel requested`
- runtime store помечает run как `cancel_requested`
- `internal/runtime/active_registry.go` отменяет активный context
- provider/tool loop должен остановиться

Это намного лучше, чем старый вариант, где poll loop блокировался на одном запросе.
