# Архитектурная документация C4

Этот каталог хранит архитектурную документацию `teamD` в C4-стиле.

## Как устроено

- [`workspace.dsl`](workspace.dsl) — каноническая модель Structurizr DSL.
- [`01-system-context.md`](01-system-context.md) — текстовое описание C4 Level 1 System Context и ссылка на view `SystemContext`.
- [`docs/`](docs/) — Markdown-документация, подключённая в Structurizr local через `!docs docs`.
- [`teamd-docs/`](teamd-docs/) — Markdown-документация, подключённая к C4-элементу `teamD Runtime` через `!docs teamd-docs`.

GitHub не рендерит Structurizr DSL напрямую. Поэтому GitHub используется для чтения текста и просмотра исходной модели, а точные диаграммы смотрятся локально через Structurizr.

## Диаграммы

1. [System Context](01-system-context.md) — граница `teamD Runtime`, оператор и внешние системы.
2. `Containers` — внутренние части `teamD Runtime`: `Operator Surfaces`, `App / Runtime Core`, `Runtime Store`.

## Документация внутри Structurizr

`workspace.dsl` подключает два уровня документации:

- [`docs/`](docs/) через workspace-level `!docs docs`: общая навигация, карта связей и терминология.
- [`teamd-docs/`](teamd-docs/) через `!docs teamd-docs` внутри `softwareSystem "teamD Runtime"`: описание конкретной системы.

После запуска Structurizr local эти разделы доступны в UI рядом с диаграммами:

- `01-overview.md` — как читать архитектурную документацию.
- `02-navigation-map.md` — как связаны views, C4-элементы и документы.
- `03-terms.md` — единая терминология: C4-элементы, бизнес-сущности и программные сущности.
- `teamd-docs/01-system-context.md` — описание view `SystemContext` с фокусом на `teamD Runtime`.
- `teamd-docs/02-runtime-boundary.md` — что входит и не входит в границу `teamD Runtime`.
- `teamd-docs/03-containers.md` — крупные внутренние части `teamD Runtime`.

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
- Текстовое описание System Context в [01-system-context.md](01-system-context.md).
- Markdown-документация для Structurizr local в [`docs/`](docs/).
- Markdown-документация `teamD Runtime` в [`teamd-docs/`](teamd-docs/).

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

Текстовая документация в `docs/current` объясняет поведение, `workspace.dsl` хранит C4-модель, а Markdown-страницы в этом каталоге объясняют, что именно смотреть в Structurizr. Папка `docs/architecture/docs` импортируется на уровне workspace, а `docs/architecture/teamd-docs` привязана к `teamD Runtime`. При изменении границ системы, внешних интеграций или основных runtime-потоков обновляйте все затронутые слои.
