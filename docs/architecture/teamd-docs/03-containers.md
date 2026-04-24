# teamD Runtime: Containers

Связанное представление: `Containers`.

Связанный C4-элемент: `teamD Runtime`.

Этот view показывает крупные внутренние части `teamD Runtime`. Это C4 Container level, а не список всех Rust-модулей.

## Containers

| Container | Назначение |
| --- | --- |
| `Operator Surfaces` | CLI, TUI, HTTP API и Telegram adapters. Эти surfaces не должны иметь отдельный runtime path. |
| `App / Runtime Core` | Каноническое выполнение chat turns, prompt assembly, provider loop, tools, approvals, schedules и inter-agent routing. |
| `Runtime Store` | SQLite metadata и payload-файлы: sessions, transcripts, runs, jobs, plans, schedules, artifacts, audit trail. |

## Основной поток

1. `Operator` работает через `Operator Surfaces`.
2. `Operator Surfaces` вызывают операции `App / Runtime Core`.
3. `App / Runtime Core` читает и пишет `Runtime Store`.
4. `App / Runtime Core` обращается к `LLM Provider`, `MCP Servers`, `GitHub Releases` и `Local Host`.
5. `Operator Surfaces` взаимодействуют с `Telegram Bot API` для updates и notifications.

## Почему surfaces отдельно от runtime

Это ключевое архитектурное правило проекта.

CLI, TUI, HTTP и Telegram должны быть тонкими интерфейсами. Они могут отличаться UX и транспортом, но не должны дублировать:

- prompt assembly;
- tool loop;
- approval handling;
- schedule wake-up;
- inter-agent routing;
- persistence semantics.

Если surface требует нового поведения, его нужно добавлять в `App / Runtime Core`, а surface должен только вызывать общий слой.

## Что будет на Component level

На следующем уровне можно раскрыть `App / Runtime Core` на components:

- `ExecutionService`;
- `PromptAssembly`;
- `provider_loop`;
- `tool execution`;
- `approval flow`;
- `scheduler`;
- `interagent`;
- `memory/session read surface`;
- `MCP registry`.
