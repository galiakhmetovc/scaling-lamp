# Agent Overview

Этот бот сейчас нужно понимать как **single-agent runtime** в одном Go-бинарнике.

Если отбросить детали, путь запроса такой:

1. Один бинарник поднимает runtime core, HTTP API и при необходимости Telegram transport.
2. Telegram присылает update.
3. `internal/transport/telegram` нормализует его и решает:
   - это slash-команда,
   - callback,
   - обычный пользовательский запрос.
4. Для обычного запроса создаётся `run`.
5. Runtime собирает prompt:
   - `AGENTS.md`
   - memory recall
   - skills prompt fragments
   - хвост истории сессии
6. Provider (`z.ai`) отвечает текстом или tool calls.
7. Если есть tool calls, они исполняются, результаты добавляются в историю, и начинается следующий round.
8. Когда модель отдаёт финальный текст, transport отправляет ответ пользователю.

Отдельно:

- CLI ходит в HTTP API
- будущий Web UI тоже должен ходить в HTTP API

## С чего читать код

В таком порядке:

1. [cmd/coordinator/main.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/main.go)
2. [cmd/coordinator/bootstrap.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/bootstrap.go)
3. [internal/transport/telegram/adapter.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/adapter.go)
4. [internal/transport/telegram/immediate_updates.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/immediate_updates.go)
5. [internal/runtime/conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
6. [internal/runtime/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
7. [internal/transport/telegram/provider_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/provider_tools.go)
8. [internal/memory/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/postgres_store.go)
9. [internal/compaction/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/service.go)

Если нужен один документ вместо прыжков по файлам, начинай с:

- [core-architecture-walkthrough.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/core-architecture-walkthrough.md)
- [http-api.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/http-api.md)
- [cli.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/cli.md)

Для phase-1 clarity docs теперь ещё важны:

- [state-machines.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/state-machines.md)
- [prompt-assembly-order.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/prompt-assembly-order.md)
- [memory-policy-cookbook.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/memory-policy-cookbook.md)
- [testing.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/testing.md)
- [operator-chat.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/operator-chat.md)

## Что не читать сначала

Не начинай с `internal/mesh`.

Mesh сохранён в репозитории, но для понимания обычного бота он не нужен. Граница описана в [mesh-boundary.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/mesh-boundary.md).
