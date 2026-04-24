# Карта связей

Этот раздел объясняет, как связаны диаграммы, C4-элементы и Markdown-документация.

## Правило

Диаграмма показывает структуру и связи. Документация объясняет смысл элементов, границы ответственности и термины.

Не дублируем диаграмму вручную в Markdown. Вместо этого каждый важный C4-элемент получает привязанную документацию через `!docs`.

## Текущая карта

| View | Главный C4-элемент | Где читать подробности |
| --- | --- | --- |
| `SystemContext` | `teamD Execution Mesh` | `system-docs/execution-mesh/01-overview.md` |
| `SystemContext` | `agentd Clients` | `system-docs/agentd-clients/01-overview.md` |
| `SystemContext` | `MCP Capability Providers` | `system-docs/mcp-capability-providers/01-overview.md` |
| `SystemContext` | `Target Resources` | `system-docs/target-resources/01-overview.md` |
| `Containers` | `agentd` | `container-docs/agentd/01-overview.md` |
| `Containers` | `Internal MCP Server` | `container-docs/internal-mcp-server/01-overview.md` |
| `Deployment` | execution nodes and mesh | `system-docs/execution-mesh/01-overview.md` |
| `TelegramDeployment` | Telegram runtime path | `../02-telegram-deployment.md` |
| Все views | Глоссарий | `docs/03-terms.md` |

## Как это выглядит в Structurizr

1. Workspace-level docs объясняют, как устроен набор документов.
2. View `SystemContext` показывает mesh и внешние boundaries.
3. Double-click по `teamD Execution Mesh` открывает выбор между zoom-in/docs.
4. View `Containers` показывает containers внутри mesh.
5. Double-click по container открывает документацию конкретного container.
6. View `Deployment` показывает execution nodes, agentd instances и MCP/resource placement.
7. View `TelegramDeployment` показывает основной practical path для Telegram: client, Bot API, execution node, local state/resources и LLM provider.

## Что добавлять дальше

Когда появится C4 Component diagram, рядом нужно добавить:

- view для конкретного container;
- документацию для важных components;
- карту связей `Component -> документ`, если components станет много.
