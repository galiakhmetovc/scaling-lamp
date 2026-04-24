# Архитектурная документация C4

Этот каталог хранит архитектурную документацию `teamD` в C4-стиле.

## Как устроено

- [`workspace.dsl`](workspace.dsl) — каноническая модель Structurizr DSL.
- [`01-system-context.md`](01-system-context.md) — текстовое описание C4 Level 1 System Context и ссылка на view `SystemContext`.

GitHub не рендерит Structurizr DSL напрямую. Поэтому GitHub используется для чтения текста и просмотра исходной модели, а точные диаграммы смотрятся локально через Structurizr.

## Диаграммы

1. [System Context](01-system-context.md) — граница `teamD Runtime`, оператор и внешние системы.

## Как посмотреть локально

Нужен Docker.

Из корня репозитория:

```bash
./docs/architecture/run-local.sh
```

Эквивалентная команда без скрипта:

```bash
docker pull structurizr/structurizr
docker run -it --rm -p 8080:8080 \
  -v "$PWD/docs/architecture:/usr/local/structurizr" \
  structurizr/structurizr local
```

Затем открыть:

```text
http://localhost:8080
```

## Что уже есть

- Представление Structurizr `SystemContext` в [`workspace.dsl`](workspace.dsl).
- Текстовое описание System Context в [01-system-context.md](01-system-context.md).

## Как проверять

Если доступен Structurizr CLI:

```bash
structurizr validate -workspace docs/architecture/workspace.dsl
structurizr inspect -workspace docs/architecture/workspace.dsl -severity error,warning
```

Если CLI нет, используйте локальный просмотр через Docker. Structurizr local парсит тот же `workspace.dsl` и покажет ошибки синтаксиса при открытии.

## Правило по диаграммам

Не ведём ручные копии C4-диаграмм в SVG или Mermaid как основной источник. Они быстро расходятся с моделью и создают вторую правду. Если понадобится статичная картинка для README или релиза, она должна быть сгенерирована из `workspace.dsl`, а не нарисована вручную.

## Правило поддержки

Текстовая документация в `docs/current` объясняет поведение, `workspace.dsl` хранит C4-модель, а Markdown-страницы в этом каталоге объясняют, что именно смотреть в Structurizr. При изменении границ системы, внешних интеграций или основных runtime-потоков обновляйте все затронутые слои.
