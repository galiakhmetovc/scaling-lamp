# Архитектурный обзор

`teamD` — локальная среда для AI-агентов общего назначения.

На архитектурном уровне система описывается через C4:

- `System Context` показывает `Operators`, `agentd Clients`, `teamD Execution Mesh`, `LLM Provider APIs`, `MCP Capability Providers` и `Target Resources`.
- `Container`-уровень описывает containers внутри `teamD Execution Mesh`: `agentd`, `Internal MCP Server`.
- `Deployment`-уровень показывает execution nodes, agentd instances, internal/external MCP и target resources.
- `TelegramDeployment` показывает практический deployment для работы оператора через Telegram.
- `Component`-уровень будет использоваться только там, где container становится слишком крупным для понимания.

## Источник правды

Каноническая архитектурная модель хранится в `workspace.dsl`.

Markdown-разделы подключены в трёх местах:

- `docs/` подключена на уровне workspace через `!docs docs`.
- `system-docs/*` подключены к конкретным software systems через `!docs`.
- `container-docs/*` подключены к конкретным containers через `!docs`.

## Как читать

1. Откройте view `SystemContext`.
2. Сделайте double-click по `teamD Execution Mesh`, чтобы перейти к его документации или к view `Containers`.
3. Откройте view `Deployment`, чтобы увидеть execution nodes и mesh-связи.
4. Откройте view `TelegramDeployment`, чтобы увидеть практический путь Telegram client -> Bot API -> `agentd` -> LLM provider.
5. В view `Containers` сделайте double-click по нужному container, чтобы открыть его документацию.
6. Используйте раздел `Глоссарий`, чтобы не смешивать C4 model, deployment, domain и runtime/code термины.

## Что не является целью этого слоя

Этот каталог не заменяет полную пользовательскую документацию в `docs/current`.

Здесь фиксируются архитектурные границы, связи, термины и решения, которые нужны для понимания устройства системы.
