# App / Runtime Core

Связанный view: `Containers`.

Связанный C4-элемент: `App / Runtime Core`.

`App / Runtime Core` — канонический слой выполнения `teamD Runtime`.

## Что входит

- `App` и app-layer operations;
- `ExecutionService`;
- prompt assembly;
- provider loop;
- structured tool execution;
- approvals;
- schedules и wake-up;
- inter-agent routing;
- memory/session read surface;
- MCP registry.

## Ответственность

Этот container отвечает за единые runtime semantics. Все surfaces должны приходить сюда, а не реализовывать собственную версию выполнения.

## Основные связи

- Получает команды от `Operator Surfaces`.
- Читает и пишет `Runtime Store`.
- Отправляет provider requests в `LLM Provider`.
- Вызывает capabilities через `MCP Servers`.
- Проверяет updates через `GitHub Releases`.
- Читает workspace и запускает processes через `Local Host`.

## Правило изменения

Изменения поведения agent turn, tools, approvals, schedules и inter-agent flows должны происходить здесь, чтобы CLI/TUI/HTTP/Telegram оставались согласованными.
