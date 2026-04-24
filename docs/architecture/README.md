# Архитектурная документация C4

Этот каталог хранит архитектурную документацию `teamD` в C4-стиле.

## Как устроено

- [`workspace.dsl`](workspace.dsl) — каноническая модель Structurizr DSL.
- [`01-system-context.md`](01-system-context.md) — текстовое описание C4 Level 1 System Context и ссылка на view `SystemContext`.
- [`02-telegram-deployment.md`](02-telegram-deployment.md) — текстовое описание deployment view для работы через Telegram.
- [`docs/`](docs/) — Markdown-документация, подключённая в Structurizr local через `!docs docs`.
- [`system-docs/`](system-docs/) — Markdown-документация, подключённая к C4-элементам `teamD Execution Mesh`, `agentd Clients`, `MCP Capability Providers`, `Target Resources`.
- [`container-docs/`](container-docs/) — Markdown-документация, подключённая к containers внутри `teamD Execution Mesh`.

GitHub не рендерит Structurizr DSL напрямую. Поэтому GitHub используется для чтения текста и просмотра исходной модели, а точные диаграммы смотрятся локально через Structurizr.

## Диаграммы

1. [System Context](01-system-context.md) — `Operators`, `agentd Clients`, `teamD Execution Mesh`, `LLM Provider APIs`, `MCP Capability Providers`, `Target Resources`.
2. `Containers` — containers внутри `teamD Execution Mesh`: `agentd`, `Internal MCP Server`.
3. `Deployment` — execution nodes, agentd instances, internal/external MCP и target resources.
4. [TelegramDeployment](02-telegram-deployment.md) — практический deployment для работы оператора через Telegram.

## Документация внутри Structurizr

`workspace.dsl` подключает несколько уровней документации:

- [`docs/`](docs/) через workspace-level `!docs docs`: общая навигация, карта связей и терминология.
- [`system-docs/*`](system-docs/) через `!docs` внутри software systems: описание конкретных систем и boundaries.
- [`container-docs/*`](container-docs/) через `!docs` внутри containers: описание конкретных runtime containers.

После запуска Structurizr local эти разделы доступны в UI рядом с диаграммами:

- `01-overview.md` — как читать архитектурную документацию.
- `02-navigation-map.md` — как связаны views, C4-элементы и документы.
- `03-terms.md` — глоссарий: C4 model, deployment, domain и runtime/code термины.
- `system-docs/*/01-overview.md` — документация конкретных software systems, открываемая через double-click по элементу.
- `container-docs/*/01-overview.md` — документация конкретных containers, открываемая через double-click по элементу.

## Как посмотреть локально

Нужен Docker.

Из корня репозитория:

```bash
./docs/architecture/run-local.sh
```

Если порт `8080` занят:

```bash
STRUCTURIZR_PORT=18080 ./docs/architecture/run-local.sh
```

Эквивалентная команда без скрипта:

```bash
docker pull structurizr/structurizr
docker run --rm --user "$(id -u):$(id -g)" -p 8080:8080 \
  -v "$PWD/docs/architecture:/usr/local/structurizr" \
  structurizr/structurizr local
```

Затем открыть:

```text
http://localhost:8080
```

## Что уже есть

- Представление Structurizr `SystemContext` в [`workspace.dsl`](workspace.dsl).
- Представление Structurizr `Containers` в [`workspace.dsl`](workspace.dsl).
- Представление Structurizr `Deployment` в [`workspace.dsl`](workspace.dsl).
- Представление Structurizr `TelegramDeployment` в [`workspace.dsl`](workspace.dsl).
- Текстовое описание System Context в [01-system-context.md](01-system-context.md).
- Текстовое описание Telegram deployment в [02-telegram-deployment.md](02-telegram-deployment.md).
- Markdown-документация для Structurizr local в [`docs/`](docs/).
- Markdown-документация systems в [`system-docs/`](system-docs/).
- Markdown-документация containers в [`container-docs/`](container-docs/).

## Как проверять

Если доступен Structurizr CLI:

```bash
structurizr validate -workspace docs/architecture/workspace.dsl
structurizr inspect -workspace docs/architecture/workspace.dsl -severity error,warning
```

Если CLI нет, используйте локальный просмотр через Docker. Structurizr local парсит тот же `workspace.dsl` и покажет ошибки синтаксиса при открытии. Служебные файлы `.structurizr/` и `workspace.json` создаются локально и игнорируются Git.

## Правило по диаграммам

Не ведём ручные копии C4-диаграмм в SVG или Mermaid как основной источник. Они быстро расходятся с моделью и создают вторую правду. Если понадобится статичная картинка для README или релиза, она должна быть сгенерирована из `workspace.dsl`, а не нарисована вручную.

## Правило поддержки

Текстовая документация в `docs/current` объясняет поведение, `workspace.dsl` хранит C4-модель, а Markdown-страницы в этом каталоге объясняют, что именно смотреть в Structurizr. Папка `docs/architecture/docs` импортируется на уровне workspace, `system-docs/*` привязаны к software systems, а `container-docs/*` привязаны к containers. При изменении границ системы, внешних интеграций или основных runtime-потоков обновляйте все затронутые слои.
