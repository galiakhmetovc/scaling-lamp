# C4 Deployment: Telegram Runtime

Источник модели: [`workspace.dsl`](workspace.dsl), представление `TelegramDeployment`.

Эта страница описывает deployment view для основного пользовательского сценария: оператор работает с `teamD` через Telegram.

Точная диаграмма не дублируется в Markdown. Смотрите view `TelegramDeployment` локально через Structurizr.

```bash
./docs/architecture/run-local.sh
```

После запуска открыть `http://localhost:8080` и выбрать view `TelegramDeployment`.

## Назначение view

`TelegramDeployment` показывает практический runtime path:

1. `Operator Device` отправляет сообщения через `Telegram Client`.
2. `Telegram Client` работает с `Telegram Bot API`.
3. `agentd` на `Execution Node` получает updates через long polling и отправляет replies/notifications через тот же Bot API.
4. `agentd` ведёт `Session`, создаёт `Job`/`Run`, вызывает `LLM Provider API` и tools.
5. Ответы, reminders и notifications возвращаются через `Telegram Bot API`.

## Что входит

| Элемент | Роль |
| --- | --- |
| `Operator Device` | Телефон или desktop оператора с Telegram client. |
| `Telegram Cloud` | Внешняя инфраструктура Telegram и `Telegram Bot API`. |
| `Execution Node` | Машина или окружение, где запущен `agentd` с Telegram long polling. |
| `Local State` | SQLite metadata, payload files, config и `.env` этого node. |
| `Local Target Resources` | Workspace, filesystem, OS processes и local tools. |
| `LLM Provider` | Внешний API модели. |

## Что намеренно не показано

- Несколько execution nodes и mesh-связи между ними.
- External MCP и external target resources.
- Детальная структура `agentd` внутри process.
- Все CLI/TUI/HTTP flows.
- Полный список Telegram commands.

Для нескольких execution nodes используйте view `Deployment`. Для внутренностей execution mesh используйте view `Containers`.

## Почему это отдельный deployment view

Общий `Deployment` отвечает на вопрос “как execution nodes образуют mesh”.

`TelegramDeployment` отвечает на другой вопрос: “что должно быть запущено и связано, чтобы оператор мог работать через Telegram”. Поэтому он проще и привязан к основному пользовательскому пути.
