# Tool Loop

## Идея

Модель не вызывает shell или filesystem напрямую. Она возвращает **tool calls**, а runtime исполняет их отдельно.

## Где это видно

- [internal/runtime/conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [internal/runtime/prompt_context_assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context_assembler.go)
- [internal/provider/provider.go](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/provider.go)
- [internal/provider/zai/client.go](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/zai/client.go)
- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)
- [internal/transport/telegram/provider_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/provider_tools.go)
- [internal/transport/telegram/memory_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_tools.go)
- [internal/transport/telegram/runtime_guards.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/runtime_guards.go)
- [internal/mcp/runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/runtime.go)

## Один полный цикл

1. Runtime собирает `provider.PromptRequest`.
2. Provider отвечает:
   - либо `Text`
   - либо `ToolCalls`
3. Если есть `ToolCalls`, runtime loop из `internal/runtime/conversation_engine.go` вызывает transport hooks для `executeTool(...)`.
4. Для guarded tools transport сначала проверяет action policy и может создать approval request вместо немедленного исполнения.
5. Tool result проходит через runtime-owned shaping:
   - маленький output остаётся inline
   - большой output может уйти в artifact offload
6. Runtime пишет observable events:
   - `approval.requested`
   - `artifact.offloaded`
   - `worker.approval_requested`
7. Tool result или synthetic approval result возвращается как `tool` message.
8. Новый round отправляется в модель.
9. Повторяется, пока модель не отдаст финальный текст.

## Как это видно оператору

Tool loop теперь не скрыт внутри Telegram.

Его можно наблюдать через:

- [`http-api.md`](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/http-api.md)
  - `GET /api/events`
  - `GET /api/events/stream`
- [`cli.md`](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/cli.md)
  - `teamd-agent events list`
  - `teamd-agent events watch`
  - `teamd-agent chat`
- [`operator-chat.md`](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/operator-chat.md)
  - readable operator console over the same event plane

## Какие инструменты реально есть

Базовые local tools живут в:

- [internal/mcp/tools/filesystem.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/tools/filesystem.go)
- [internal/mcp/tools/shell.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/tools/shell.go)

Дополнительные tool surfaces:

- `skills.list`
- `skills.read`
- `activate_skill`
- `memory_search`
- `memory_read`

И ещё важная runtime-policy поверхность:

- artifact offload
  - tool output может быть превращён не в длинный inline blob, а в `artifact_ref + preview`

## Guardrails

Tool loop не должен крутиться бесконечно.

Сейчас есть:

- provider round timeout
- repeated tool call breaker
- advisory stop policy
- explicit action policy for risky tools
- approval records and approve/reject decisions through API, CLI, and Telegram
- cancellation checks

Если ты хочешь понять, почему run остановился или не остановился, смотри именно эти guardrails.

## Как теперь читать код

- `adapter.go` — вход, dispatch, orchestration вокруг run
- `conversation.go` — Telegram-адаптер над core runtime loop
- `internal/runtime/conversation_engine.go` — основной LLM/tool loop
- `internal/runtime/prompt_context_assembler.go` — runtime-owned injection of workspace/recall/skills fragments
- `provider_tools.go` — какие tools вообще объявляются модели и как generic tools исполняются
- `memory_tools.go` — model-facing memory tools
- `runtime_guards.go` — advisory stop, repeated-call breaker, shared loop helpers
- `internal/approvals/service.go` — approval records и callback FSM
- [artifact-offload.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/artifact-offload.md) — почему большие tool outputs не должны жить inline
