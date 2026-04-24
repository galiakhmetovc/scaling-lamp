# Глоссарий

Этот глоссарий фиксирует общий язык для архитектурной документации `teamD`.

Правило: в русском тексте обычные объяснения пишем на русском, а имена C4-элементов, runtime-сущностей и технические идентификаторы оставляем в каноническом английском виде.

## Слои терминов

| Слой | Что описывает | Пример |
| --- | --- | --- |
| C4 model | Архитектурные элементы на диаграммах. | `teamD Execution Mesh`, `agentd Clients` |
| Deployment | Где живут процессы, узлы и ресурсы. | `Execution Node`, `External MCP Server` |
| Domain | Понятия предметной области для оператора и агента. | `Agent`, `Session`, `Schedule` |
| Runtime/code | Программные сущности и внутренние механизмы. | `Run`, `Job`, `Tool`, `Artifact` |

## C4 model

| Термин | Тип | Каноническое значение |
| --- | --- | --- |
| `Operators` | Person | Люди или automation-участники, которые работают с агентами, читают результаты, подтверждают действия и управляют runtime. |
| `agentd Clients` | Software System | Клиенты взаимодействия: CLI, TUI, HTTP clients и Telegram-mediated client flow. Клиенты отправляют команды и показывают состояние, но не исполняют агентскую работу. |
| `teamD Execution Mesh` | Software System | Основная система: один или несколько execution nodes с `agentd`, где выполняются sessions, jobs, tools, schedules, inter-agent flows и provider calls. |
| `LLM Provider APIs` | Software System | Внешние API моделей, которые принимают provider requests и возвращают assistant text, reasoning и structured tool calls. |
| `MCP Capability Providers` | Software System | Capability boundary для internal/external MCP providers, которые дают `agentd` tools, resources и prompts. |
| `Target Resources` | Software System | Ресурсы, на которые `agentd` или MCP tools могут воздействовать: workspace, filesystem, OS processes, Git repos, APIs, infrastructure, databases, cloud resources. |
| `agentd` | Container | Daemon/runtime process внутри execution node: HTTP API, canonical runtime, provider loop, tools, approvals, schedules, persistence и inter-agent routing. |
| `Internal MCP Server` | Container | MCP server внутри execution node или управляемый тем же окружением; предоставляет локальные tools/resources/prompts. |

## Deployment

| Термин | Каноническое значение |
| --- | --- |
| `Runtime Mesh` | Deployment environment, где показаны execution nodes, agentd instances, internal/external MCP и target resources. |
| `Execution Node` | Машина, VM, контейнер, WSL-окружение или сервер, где запущен `agentd` и есть локальное состояние. |
| `agentd instance` | Конкретный запущенный процесс `agentd` на конкретном execution node. |
| `agentd mesh` | Связи между execution nodes или agentd instances для remote delegation, inter-agent routing и будущего A2A. |
| `Local Target Resources` | Ресурсы, локальные для execution node: workspace, filesystem, OS processes, local tools. |
| `External Target Resources` | Ресурсы вне execution nodes: GitHub, cloud, DB, Kubernetes, external APIs, infrastructure. |
| `External MCP Server` | MCP server вне execution nodes: отдельный сервис, remote tool gateway или shared capability provider. |
| `External MCP Endpoint` | Конкретная точка подключения к external MCP server. |

## Interaction и transports

| Термин | Каноническое значение |
| --- | --- |
| `CLI` | Командный клиент `agentd`. Клиентский вход в execution mesh. |
| `TUI` | Terminal UI client. Показывает sessions, transcript, approvals, agents, schedules и вызывает те же runtime operations, что CLI/HTTP. |
| `HTTP API` | API daemon process. Через него клиенты отправляют команды и читают состояние. |
| `Telegram-mediated client flow` | Пользовательский flow через Telegram: Telegram client и Bot API являются transport/client path, а выполнение остаётся в execution mesh. |
| `Telegram Bot API` | Внешний API Telegram. На System Context не выделяется отдельной главной системой, если обсуждаем общую модель клиентов; при детализации Telegram может быть отдельной external dependency. |
| `Surface` | Разговорный термин для UI/adapter слоя. В C4-глоссарии вместо него используем `agentd Clients`, если речь о клиентах, или конкретный adapter/module, если речь о коде. |

## Domain

| Термин | Каноническое значение |
| --- | --- |
| `Agent` | Настроенный AI-участник, который действует в session согласно profile, инструкциям, model settings и tool policy. |
| `Agent Profile` | Конфигурация агента: id, имя, template, model, prompts, tools, policies, reasoning/think settings. |
| `Session` | Диалоговый контекст агента: выбранный agent profile, transcript, runtime-настройки, планы, summaries и связанные jobs/runs. |
| `Message` | Запись общения в session: user/system/assistant/tool/inter-agent content. |
| `Transcript` | Упорядоченная история сообщений session, которую можно показывать оператору и использовать для prompt assembly. |
| `Turn` | Один пользовательский или системный шаг общения, который запускает обработку агентом. |
| `Schedule` | Правило или отложенное событие, по которому агент должен продолжить работу позже или регулярно. |
| `Approval` | Решение оператора разрешить или отклонить действие, если policy требует ручного подтверждения. |
| `Inter-agent chain` | Цепочка сообщений между агентами с `chain_id`, `origin_session_id`, hop count и лимитом продолжений. |
| `Hop` | Один переход сообщения от одного агента к другому внутри inter-agent chain. |

## Runtime/code

| Термин | Каноническое значение |
| --- | --- |
| `App` | Прикладной слой, который объединяет config, runtime services, persistence и клиентские adapters. |
| `Daemon` | Долго живущий процесс `agentd`, который обслуживает HTTP API, background jobs и runtime operations. |
| `Run` | Одна попытка выполнения agent turn или background job. Run хранит состояние попытки, tool calls, результаты и usage. |
| `Job` | Долговечная единица работы: chat turn, schedule wake-up, delegation, tool execution или другая фоновая задача. |
| `Tool` | Структурированная возможность, которую модель может вызвать через canonical tool surface. |
| `Built-in Tool` | Tool, реализованный внутри `agentd`: filesystem, execution, planning, schedules, inter-agent operations и другие runtime capabilities. |
| `MCP Tool` | Tool, предоставленный MCP capability provider. |
| `Artifact` | Вынесенный payload: большой tool output, файл, результат выполнения или другое содержимое, которое не надо держать целиком в prompt. |
| `Payload file` | Файл с содержимым artifact или крупного runtime payload рядом с SQLite metadata. |
| `Runtime Store` | Implementation-level persistence: SQLite metadata и связанные payload-файлы. Это не C4 container в текущей модели. |
| `Prompt Assembly` | Сборка prompt в каноническом порядке: `SYSTEM.md`, `AGENTS.md`, `SessionHead`, `Plan`, `ContextSummary`, offload refs, uncovered transcript tail. |
| `Provider Loop` | Цикл взаимодействия с LLM provider: отправить request, принять assistant output/tool calls, выполнить tools, продолжить до завершения или остановки. |
| `ContextSummary` | Сжатое представление старого контекста session, которое добавляется в prompt вместо полного transcript. |
| `Offload ref` | Ссылка на artifact или payload, который не включается в prompt целиком и читается явно через artifact tools. |
| `Plan` | Структурированный план работы агента: tasks, statuses, notes и lint/snapshot operations. |
| `Audit trail` | Журнал runtime-событий для диагностики: startup, HTTP requests, jobs, tool runs, errors. |

## Поток end-to-end

| Шаг | Термины |
| --- | --- |
| 1 | `Operators` работают через `agentd Clients`. |
| 2 | `agentd Clients` отправляют команду или сообщение в `teamD Execution Mesh`. |
| 3 | Внутри mesh конкретный `agentd instance` создаёт или продолжает `Session`, `Job` и `Run`. |
| 4 | `agentd` собирает prompt через `Prompt Assembly` и вызывает `LLM Provider APIs`. |
| 5 | Модель возвращает assistant text и structured tool calls. |
| 6 | `agentd` выполняет `Built-in Tools` или вызывает `MCP Capability Providers`. |
| 7 | Tools воздействуют на `Target Resources` напрямую или через MCP. |
| 8 | Результаты сохраняются в `Runtime Store`, крупные payload выносятся в `Artifacts`, ответ возвращается клиенту. |

## Канонические формулировки

| Лучше писать | Не писать | Почему |
| --- | --- | --- |
| `teamD Execution Mesh` | `teamD Runtime` как C4 boundary | Система теперь моделируется как mesh execution nodes, а не один локальный runtime-блок. |
| `agentd Clients` | `Operator Surfaces` как C4 container | Клиенты могут быть локальными, удалёнными или mediated через Telegram; они не являются container внутри mesh. |
| `Execution Node` | `Local Host` | Узел может быть локальным, удалённым, временным или серверным. |
| `MCP Capability Providers` | `MCP Servers` без уточнения | На System Context важна capability boundary; на Deployment уточняем internal/external MCP. |
| `Target Resources` | `Local Host` как всё внешнее | Ресурсы могут быть локальными и внешними, прямыми и доступными через MCP. |
| `Telegram-mediated client flow` | `Telegram Surface` внутри mesh | Telegram — внешний transport/client path, выполнение остаётся в execution mesh. |
| `AI-агенты общего назначения` | `кодирующие агенты` | `teamD` не ограничен только coding use case. |

## Инварианты терминологии

- `agentd Clients` не исполняют агентскую работу; они вызывают execution mesh.
- `teamD Execution Mesh` исполняет sessions, jobs, tools, schedules и provider calls.
- `agentd` — container/process внутри execution node, а не название всей системы на C4 System Context.
- `Target Resources` не являются частью execution mesh, даже если находятся локально на том же node.
- MCP может быть internal или external, но на System Context это всегда `MCP Capability Providers`.
- `Runtime Store` остаётся implementation-level термином и не должен возвращаться как отдельный C4 container без отдельного архитектурного решения.
