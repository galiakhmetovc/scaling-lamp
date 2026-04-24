# Термины и сущности

Этот раздел разделяет четыре слоя терминов:

- C4-элементы: то, что видно на архитектурных диаграммах.
- Deployment-термины: где живут процессы и ресурсы.
- Бизнес-сущности: понятия предметной области для пользователя и оператора.
- Программные сущности: структуры и компоненты внутри кода.

## C4-элементы

| Термин | Слой | Значение |
| --- | --- | --- |
| `Operators` | Person | Люди или automation-участники, которые работают с агентами и управляют runtime. |
| `agentd Clients` | Software System | Клиенты взаимодействия: CLI, TUI, HTTP clients и Telegram-mediated client flow. |
| `teamD Execution Mesh` | Software System | Основная система: execution nodes с agentd instances, где выполняется агентская работа. |
| `LLM Provider APIs` | Software System | Внешние API моделей. |
| `MCP Capability Providers` | Software System | Internal/external providers, которые дают tools/resources/prompts. |
| `Target Resources` | Software System | Ресурсы, на которые agentd или MCP tools могут воздействовать. |

## C4 containers внутри `teamD Execution Mesh`

| Термин | Значение |
| --- | --- |
| `agentd` | Daemon/runtime process внутри execution node. |
| `Internal MCP Server` | MCP server внутри execution node или управляемый тем же окружением. |

## Deployment-термины

| Термин | Значение |
| --- | --- |
| `Execution Node` | Машина или окружение, где запущен `agentd` и локальное состояние. |
| `agentd mesh` | Связи между agentd instances: remote delegation, inter-agent routing, future A2A. |
| `External MCP Server` | MCP server вне execution nodes. |
| `External Target Resources` | Ресурсы вне execution nodes: GitHub, cloud, DB, Kubernetes, external APIs, infrastructure. |

## Бизнес-сущности

| Термин | Значение |
| --- | --- |
| `Agent` | Настроенный AI-участник с профилем, моделью, инструкциями и разрешёнными инструментами. |
| `Session` | Диалоговый контекст агента: сообщения, transcript, выбранный agent profile, runtime-настройки. |
| `Schedule` | Правило, по которому агент должен продолжить работу позже или регулярно. |
| `Approval` | Решение оператора разрешить или отклонить действие, если policy требует ручного подтверждения. |
| `Inter-agent chain` | Цепочка сообщений между агентами с ограничением по hop count. |

## Программные сущности

| Термин | Значение |
| --- | --- |
| `App` | Прикладной слой, который объединяет config, runtime services, persistence и surfaces. |
| `Run` | Одна попытка выполнения agent turn или background job. |
| `Job` | Долговечная единица работы: chat turn, schedule wake-up, delegate, tool execution или другая фоновая задача. |
| `Tool` | Структурированная возможность, которую модель может вызвать через canonical tool surface. |
| `Artifact` | Вынесенный payload: большой tool output, файл, результат выполнения или другое содержимое, которое не надо держать целиком в prompt. |
| `Agent Profile` | Конфигурация агента: имя, template, model, prompts, tools, policies. |

## Правило терминологии

В русской документации обычный текст пишется на русском. Английскими остаются имена сущностей, C4-термины и технические идентификаторы: `Session`, `Run`, `Job`, `Tool`, `Artifact`, `Agent Profile`, `System Context`.
