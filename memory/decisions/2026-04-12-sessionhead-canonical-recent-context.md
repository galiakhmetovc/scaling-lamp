# 2026-04-12 — SessionHead as canonical recent-context layer

## Проблема

Сессия доступна агенту, но в неудобной и слишком сырой форме:

- transcript
- checkpoint
- continuity
- replay
- events
- artifacts

Эти источники существуют, но у runtime нет одного малого канонического объекта:

- что сейчас является правдой по этой сессии
- что только что было сделано
- какой результат считается текущим итогом

Из-за этого агент между runs может выглядеть так, будто "всё забыл", хотя нужные данные физически ещё существуют.

## Решение

Нужен отдельный persisted слой:

- `SessionHead`

Это не дальняя memory и не transcript, а канонический recent-context record на сессию.

## Минимальный состав

- `session_id`
- `last_completed_run_id`
- `current_goal`
- `last_result_summary`
- `resolved_entities`
- `recent_artifact_refs`
- `open_loops`
- `current_project`

## Правила использования

1. После каждого meaningful run runtime обновляет `SessionHead`
2. Prompt assembly сначала inject-ит `SessionHead`
3. Только потом идут:
   - continuity/checkpoint
   - memory recall
4. Команды:
   - `продолжай`
   - `оформи как проект`
   - `сохрани как кейс`
   должны опираться на `SessionHead`, а не на `memory.search`

## Зачем

Это устраняет корневую проблему:

- не "плохой поиск по памяти"
- не "плохой project capture"
- а отсутствие session-local source of truth между runs

## Связанные задачи

- `teamD-5ic`
- `teamD-y86`
