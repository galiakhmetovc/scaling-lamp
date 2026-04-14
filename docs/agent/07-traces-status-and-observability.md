# Traces, Status, And Observability

## LLM trace

Для каждого run можно писать raw provider trace на диск.

Реализация:

- [internal/llmtrace/trace.go](/home/admin/AI-AGENT/data/projects/teamD/internal/llmtrace/trace.go)

Что туда попадает:

- assembled messages
- request config
- tools
- provider request body
- provider response body
- parsed response

Это лучший источник правды, если Telegram показал не то, что реально вернула модель.

## Status card

Telegram status card — это UI-слой.

Он показывает:

- stage
- elapsed time
- round
- waiting state
- current tool
- context estimate

Реализация:

- [internal/transport/telegram/status_card.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/status_card.go)
- [internal/transport/telegram/commands.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/commands.go)

## Важный operational principle

Ошибка status card не должна убивать run.

Именно поэтому:

- edit throttling есть
- Telegram `429` обрабатывается отдельно
- `message is not modified` не считается фатальной ошибкой

## Structured logs

Runtime пишет важные события как structured logs:

- run start
- run complete/fail/abort
- guard trigger
- provider timeout

Эти логи нужны, когда trace ещё не успел записаться или проблема не в provider, а в orchestration/runtime.
