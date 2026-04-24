# C4 Level 1: System Context

Источник модели: [`workspace.dsl`](workspace.dsl), представление `SystemContext`.

Эта страница описывает System Context, который задан в Structurizr DSL.

Точная диаграмма не дублируется в Markdown. Смотрите view `SystemContext` локально через Structurizr, чтобы не поддерживать вторую ручную картинку рядом с канонической моделью.

```bash
./docs/architecture/run-local.sh
```

После запуска открыть `http://localhost:8080` и выбрать view `SystemContext`.

Если порт `8080` занят, запустить с другим портом:

```bash
STRUCTURIZR_PORT=18080 ./docs/architecture/run-local.sh
```

## Граница системы

`teamD Execution Mesh` — основная система. Это один или несколько execution nodes с `agentd`, которые исполняют агентскую работу и могут образовывать mesh.

## Люди и внешние системы

| C4-элемент | Название | Роль |
| --- | --- | --- |
| Person | `Operators` | Люди или automation-участники: работают с агентами, читают результаты, подтверждают действия. |
| Software System | `agentd Clients` | CLI, TUI, HTTP clients и Telegram-mediated client flow. Клиенты не исполняют агентскую работу. |
| Software System | `teamD Execution Mesh` | Execution nodes с `agentd`, где выполняются sessions, jobs, tools, schedules и provider calls. |
| Software System | `LLM Provider APIs` | Внешние API моделей. |
| Software System | `MCP Capability Providers` | Internal/external MCP providers, которые дают tools/resources/prompts. |
| Software System | `Target Resources` | Ресурсы, на которые воздействуют agentd или MCP tools. |

## Основные связи

| Откуда | Куда | Смысл |
| --- | --- | --- |
| `Operators` | `agentd Clients` | Работают через CLI, TUI, HTTP или Telegram. |
| `agentd Clients` | `teamD Execution Mesh` | Отправляют команды, сообщения и читают состояние. |
| `teamD Execution Mesh` | `LLM Provider APIs` | Отправляет provider requests. |
| `teamD Execution Mesh` | `MCP Capability Providers` | Ищет и вызывает capabilities. |
| `teamD Execution Mesh` | `Target Resources` | Воздействует напрямую через built-in tools. |
| `MCP Capability Providers` | `Target Resources` | Воздействуют через MCP tools. |

## Что не показано на этом уровне

- Execution nodes и agentd instances внутри mesh.
- Internal/external MCP placement.
- Конкретные containers внутри `agentd`.
- Детальный поток `chat turn`.

Следующие уровни: view `Containers` для `teamD Execution Mesh` и view `Deployment` для execution nodes.
