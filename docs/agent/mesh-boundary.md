# Mesh Boundary

`internal/mesh` сохранён в проекте, но не является частью основного пути понимания single-agent бота.

## Правило чтения кода

Если вы хотите понять, как работает обычный Telegram-бот, **игнорируйте mesh**.

Сначала читайте:

1. `cmd/coordinator/main.go`
2. `internal/transport/telegram`
3. `internal/runtime`
4. `internal/provider`
5. `internal/memory`
6. `internal/compaction`

## Когда mesh вообще включается

Mesh wiring создаётся только если:

- `TEAMD_MESH_ENABLED=true`
- и заданы остальные mesh-параметры (`TEAMD_AGENT_ID`, `TEAMD_MESH_LISTEN_ADDR`, `TEAMD_MESH_REGISTRY_DSN`, ...).

По умолчанию mesh выключен.

## Что это даёт

- Основной бот можно читать как single-agent runtime.
- Код mesh не выкинут и может развиваться отдельно.
- Внутренние mesh-концепции не должны быть обязательны для понимания `/status`, `/cancel`, tool loop, memory, compaction или launcher.
