# Архитектурная документация C4

Этот каталог хранит архитектурную документацию `teamD` в C4-стиле.

## Как устроено

- [`workspace.dsl`](workspace.dsl) — каноническая модель Structurizr DSL.
- [`01-system-context.md`](01-system-context.md) — C4 Level 1 System Context для чтения в GitHub.
- [`assets/system-context.svg`](assets/system-context.svg) — SVG-представление System Context с ручным orthogonal routing.

GitHub не рендерит Structurizr DSL напрямую, поэтому Markdown-страницы содержат версии ключевых представлений, которые можно читать прямо в GitHub. Structurizr DSL остаётся строгой моделью, а SVG нужен для аккуратного просмотра прямо в репозитории без локальных рендереров.

## Диаграммы

1. [System Context](01-system-context.md) — граница `teamD Runtime`, оператор и внешние системы.

## Как посмотреть локально

Нужен Docker.

```bash
docker run -it --rm -p 8080:8080 \
  -v "$PWD/docs/architecture:/usr/local/structurizr" \
  structurizr/structurizr local
```

Затем открыть:

```text
http://localhost:8080
```

## Что уже есть

- Представление Structurizr `SystemContext`.
- SVG-страница [01-system-context.md](01-system-context.md), которую GitHub рендерит прямо в Markdown.

## Как проверять

Если доступен Structurizr CLI:

```bash
structurizr validate -workspace docs/architecture/workspace.dsl
structurizr inspect -workspace docs/architecture/workspace.dsl -severity error,warning
```

Если CLI нет, используйте локальный просмотр через Docker. Structurizr local парсит тот же `workspace.dsl` и покажет ошибки синтаксиса при открытии.

## Правило поддержки

Текстовая документация в `docs/current` объясняет поведение, `workspace.dsl` хранит C4-модель, а Markdown/SVG-страницы в этом каталоге дают представление, которое GitHub рендерит прямо в репозитории. При изменении границ системы, внешних интеграций или основных runtime-потоков обновляйте все затронутые слои.
