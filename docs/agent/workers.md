# Workers

`Workers` в `teamD` — это managed local subagents без mesh.

Это не background job и не удалённый агент.

Это supervised local LLM runtime с:

- own worker session
- own inbox/outbox
- own run lifecycle
- isolated worker transcript

## Главное различие

### Job

- запускает команду
- даёт logs/status/cancel
- не умеет разговаривать

### Worker

- принимает `message`
- запускает свой local agent loop
- возвращает `messages` и `events`
- может быть опрошен через `wait`

## Почему worker memory изолирована

По умолчанию worker не пишет свой шум в общую память проекта.

У worker есть:

- свой `worker_chat_id`
- свой `worker_session_id`
- свой transcript

Это важно, потому что:

- временные рассуждения worker не должны автоматически становиться общей истиной
- проще дебажить, что сделал именно worker
- это готовит архитектуру к будущему mesh

## Текущая модель

Сейчас worker строится поверх существующего single-agent runtime.

То есть:

- worker не отдельный бинарник
- worker не отдельный процесс
- worker использует тот же runtime core
- но живёт в отдельной synthetic session

Практически:

- `worker_chat_id` делается отрицательным synthetic id
- transcript хранится отдельно от owner chat
- worker имеет свой `last_run_id`
- после завершения run worker пишет canonical `handoff`

## Handoff contract

Worker возвращает родителю не только transcript, но и структурированный handoff:

- `summary`
- `artifacts`
- `promoted_facts`
- `open_questions`
- `recommended_next_step`

В baseline-версии handoff строится так:

- `summary` = последний meaningful assistant message worker'а
- `artifacts` = `artifact_refs` последнего worker run
- `promoted_facts` пока пустые по умолчанию
- `open_questions` пока пустые по умолчанию
- `recommended_next_step` пока пустой по умолчанию

Это уже лучше, чем тащить весь raw transcript обратно в parent path.

## Где смотреть код

- [workers_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/workers_service.go)
- [types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/types.go)
- [store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [execution_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/execution_service.go)
- [conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)

## Базовый lifecycle

1. `spawn`
   - создаёт `WorkerRecord`
   - создаёт isolated worker session
2. `message`
   - стартует detached local run для worker
3. `wait`
   - неблокирующе возвращает:
     - current worker state
     - current handoff if it already exists
     - новые messages
     - новые events
4. `close`
   - закрывает worker и больше не принимает новые messages

## Почему `wait` неблокирующий

Это сознательный контракт.

CLI, API и будущий UI должны уметь:

- poll status кусками
- читать частичный progress
- не висеть в блокирующем вызове

Поэтому `wait` у нас ближе к:

- `poll with cursors`

а не к:

- `block until done`

## Как тестировать

```bash
curl -X POST http://127.0.0.1:18081/api/workers \
  -H 'Content-Type: application/json' \
  -d '{"chat_id":1001,"session_id":"1001:default","prompt":"say hi"}'

teamd-agent workers list 1001
teamd-agent workers show worker-1
teamd-agent workers wait worker-1
teamd-agent workers handoff worker-1
teamd-agent workers close worker-1
```
