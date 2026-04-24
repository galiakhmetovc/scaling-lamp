# Карта связей

Этот раздел объясняет, как связаны диаграммы, C4-элементы и Markdown-документация.

## Правило

Диаграмма показывает структуру и связи. Документация объясняет смысл элементов, границы ответственности и термины.

Не дублируем диаграмму вручную в Markdown. Вместо этого каждый важный C4-элемент получает привязанную документацию через `!docs`.

## Текущая карта

| View | Главный C4-элемент | Где читать подробности |
| --- | --- | --- |
| `SystemContext` | `teamD Runtime` | `teamd-docs/01-system-context.md` |
| `SystemContext` | `teamD Runtime` boundary | `teamd-docs/02-runtime-boundary.md` |
| `Containers` | `teamD Runtime` internals | `teamd-docs/03-containers.md` |
| `Containers` | `Operator Surfaces` | `container-docs/operator-surfaces/01-overview.md` |
| `Containers` | `App / Runtime Core` | `container-docs/app-runtime-core/01-overview.md` |
| `Containers` | `Runtime Store` | `container-docs/runtime-store/01-overview.md` |
| Все views | Термины и сущности | `docs/03-terms.md` |

## Как это выглядит в Structurizr

1. Workspace-level docs объясняют, как устроен набор документов.
2. View `SystemContext` показывает систему и внешние зависимости.
3. Double-click по `teamD Runtime` открывает выбор между zoom-in/docs.
4. View `Containers` показывает внутренние containers.
5. Double-click по container открывает документацию конкретного container.

## Что добавлять дальше

Когда появится C4 Component diagram, рядом нужно добавить:

- view для конкретного container;
- документацию для важных components;
- карту связей `Component -> документ`, если components станет много.
