# Request Lifecycle

## Один запрос от Telegram до ответа

Это самый короткий путь, который нужно понять новичку.

## 1. Telegram приносит update

Файл:

- [internal/transport/telegram/adapter.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/adapter.go)

Что происходит:

- `Poll(...)` читает `getUpdates`
- `Dispatch(...)` решает, это callback, slash-команда или обычный user run
- для обычного сообщения создаётся или продолжается session run

## 2. Adapter готовит run

Файл:

- [internal/transport/telegram/adapter.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/adapter.go)

Ключевые шаги:

- проверить, не занят ли chat другим run
- записать `user` message в session store
- создать trace collector
- подготовить status card

Это ещё не LLM loop. Это orchestration вокруг него.

## 3. Conversation loop собирает prompt

Файл:

- [internal/runtime/conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [internal/runtime/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)
- [internal/transport/telegram/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/prompt_context.go)

Ключевые шаги:

- `conversation.go` собирает transport-specific hooks
- `internal/runtime/prompt_context.go` делает core path:
  - загрузить raw session history
  - при необходимости запустить compaction
  - взять checkpoint
  - собрать prompt через `compaction.AssemblePrompt(...)`
- `internal/transport/telegram/prompt_context.go` добавляет transport-specific fragments:
  - `AGENTS.md` context
  - memory recall
  - skills catalog
  - active skill prompts

## 4. Provider получает round

Файлы:

- [internal/provider/provider.go](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/provider.go)
- [internal/provider/zai/client.go](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/zai/client.go)

Что происходит:

- runtime вызывает `provider.Generate(...)`
- если round зависает слишком долго, его обрывает `providerRoundTimeout`

## 5. Если модель хочет tool

Файлы:

- [internal/runtime/conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)
- [internal/transport/telegram/tool_runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/tool_runtime.go)
- [internal/mcp/runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/runtime.go)

Что происходит:

- provider возвращает `ToolCalls`
- runtime пишет `assistant` message с tool calls
- `executeTool(...)` запускает нужный tool
- результат сохраняется как `tool` message
- loop идёт в следующий round

## 6. Если модель вернула финальный текст

Файлы:

- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)
- [internal/transport/telegram/telegram_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/telegram_api.go)

Что происходит:

- финальный `assistant` текст пишется в session store
- continuity обновляется
- trace сохраняется на диск
- Telegram получает итоговый ответ

## 7. Что сохраняется по пути

Файлы:

- [internal/transport/telegram/memory_runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_runtime.go)
- [internal/runtime/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)
- [internal/memory/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/postgres_store.go)

Сохраняется:

- raw messages
- run record
- working state:
  - checkpoint
  - continuity
- searchable memory documents
- llm trace

## 8. Что читать, если нужно понять поведение

- Не начинай с всего проекта.
- Сначала:
  - `main.go`
  - `bootstrap.go`
  - `adapter.go`
  - `internal/runtime/conversation_engine.go`
  - `conversation.go`
  - `tool_runtime.go`
  - `memory_runtime.go`

Этого уже достаточно, чтобы понять почти весь single-agent path.
