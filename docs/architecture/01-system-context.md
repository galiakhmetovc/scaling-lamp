# C4 Level 1: System Context

Источник модели: [`workspace.dsl`](workspace.dsl), view `SystemContext`.

Эта страница дублирует System Context из Structurizr DSL в Mermaid, потому что GitHub рендерит Mermaid прямо в Markdown без локальных renderer'ов.

```mermaid
flowchart LR
  operator["Operator<br/><small>User, developer, or administrator</small>"]
  teamd["teamD Runtime<br/><small>Local general-purpose AI-agent runtime</small>"]
  llm["LLM Provider<br/><small>Text, reasoning, tool calls</small>"]
  telegram["Telegram Bot API<br/><small>Chat access and notifications</small>"]
  mcp["MCP Servers<br/><small>External tools, resources, prompts</small>"]
  github["GitHub Releases<br/><small>Runtime update source</small>"]
  host["Local Host<br/><small>Workspace, OS, SQLite, payload files</small>"]

  operator -->|"Works with agents through CLI, TUI, Telegram, HTTP"| teamd
  operator -->|"Runs agentd, edits config, opens local views"| host
  teamd -->|"Sends provider requests"| llm
  teamd -->|"Polls updates and sends messages"| telegram
  teamd -->|"Discovers and invokes capabilities"| mcp
  teamd -->|"Checks and downloads updates"| github
  teamd -->|"Reads/writes files, runs processes, stores state"| host
```

## System Boundary

`teamD Runtime` — локальная среда для AI-агентов общего назначения. На этом уровне она считается одной системой: внутренние части (`agentd`, daemon, TUI backend, Telegram worker, persistence, provider loop) будут раскрыты на C4 Container и Component диаграммах.

## Люди и внешние системы

| C4 element | Название | Роль |
| --- | --- | --- |
| Person | `Operator` | Пользователь, разработчик или администратор: общается с агентами, читает результаты, подтверждает действия, управляет runtime. |
| Software System | `LLM Provider` | Внешний API модели: принимает provider request и возвращает текст, reasoning, tool calls. |
| Software System | `Telegram Bot API` | Внешний API Telegram: long polling, команды, pairing, входящие и исходящие сообщения. |
| Software System | `MCP Servers` | Внешние или локальные MCP-серверы: дополнительные tools, resources, prompts. |
| Software System | `GitHub Releases` | Источник release-артефактов для self-update. |
| Software System | `Local Host` | Машина или сервер оператора: workspace, OS processes, terminal, SQLite DB, payload-файлы. |

## Основные связи

| From | To | Meaning |
| --- | --- | --- |
| `Operator` | `teamD Runtime` | Работает с агентами через CLI, TUI, Telegram и HTTP. |
| `Operator` | `Local Host` | Запускает `agentd`, редактирует конфиг, открывает локальные views. |
| `teamD Runtime` | `LLM Provider` | Отправляет provider requests и получает ответы модели/tool calls. |
| `teamD Runtime` | `Telegram Bot API` | Получает updates и отправляет replies/notifications. |
| `teamD Runtime` | `MCP Servers` | Ищет и вызывает внешние capabilities. |
| `teamD Runtime` | `GitHub Releases` | Проверяет и скачивает обновления. |
| `teamD Runtime` | `Local Host` | Читает/пишет workspace, запускает процессы, хранит persistent state. |

## Что не показано на этом уровне

- Внутренние контейнеры `teamD Runtime`.
- Runtime data model: `Session`, `Run`, `Job`, `Tool`, `Artifact`.
- Конкретные HTTP endpoints и TUI screens.
- Детальный chat turn flow.

Следующий уровень: C4 Container diagram для `teamD Runtime`.
