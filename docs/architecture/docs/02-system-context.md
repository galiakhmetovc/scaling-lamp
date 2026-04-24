# System Context

Источник модели: `workspace.dsl`, view `SystemContext`.

`SystemContext` показывает `teamD Runtime` как одну систему и фиксирует её связи с оператором и внешними системами.

## Граница системы

`teamD Runtime` — локальная среда для AI-агентов общего назначения.

На этом уровне внутренние части системы не раскрываются. `agentd`, daemon, TUI backend, Telegram worker, persistence и provider loop будут показаны на следующих C4-уровнях.

## Участники

| C4-элемент | Название | Роль |
| --- | --- | --- |
| Person | `Operator` | Пользователь, разработчик или администратор: общается с агентами, читает результаты, подтверждает действия, управляет runtime. |
| Software System | `teamD Runtime` | Локальная среда для AI-агентов общего назначения. |
| Software System | `LLM Provider` | Внешний API модели: принимает provider requests и возвращает текст, reasoning и tool calls. |
| Software System | `Telegram Bot API` | Внешний API Telegram: long polling, команды, pairing, входящие и исходящие сообщения. |
| Software System | `MCP Servers` | Внешние или локальные MCP-серверы: дополнительные tools, resources и prompts. |
| Software System | `GitHub Releases` | Источник release-артефактов для self-update. |
| Software System | `Local Host` | Машина или сервер оператора: workspace, процессы OS, terminal, SQLite DB, payload-файлы. |

## Основные связи

| Откуда | Куда | Смысл |
| --- | --- | --- |
| `Operator` | `teamD Runtime` | Работает с агентами через CLI, TUI, Telegram и HTTP. |
| `Operator` | `Local Host` | Запускает `agentd`, редактирует config, открывает локальные представления. |
| `teamD Runtime` | `LLM Provider` | Отправляет provider requests и получает ответы модели и tool calls. |
| `teamD Runtime` | `Telegram Bot API` | Получает updates и отправляет replies/notifications. |
| `teamD Runtime` | `MCP Servers` | Ищет и вызывает внешние возможности. |
| `teamD Runtime` | `GitHub Releases` | Проверяет и скачивает обновления. |
| `teamD Runtime` | `Local Host` | Читает и пишет workspace, запускает процессы, хранит состояние. |

## Не показано на этом уровне

- Внутренние контейнеры `teamD Runtime`.
- Модель данных runtime: `Session`, `Run`, `Job`, `Tool`, `Artifact`.
- Конкретные HTTP endpoints и экраны TUI.
- Детальный поток `chat turn`.
