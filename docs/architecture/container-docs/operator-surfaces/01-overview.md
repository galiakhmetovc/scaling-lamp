# Operator Surfaces

Связанный view: `Containers`.

Связанный C4-элемент: `Operator Surfaces`.

`Operator Surfaces` — это слой пользовательских и интеграционных интерфейсов над `teamD Runtime`.

## Что входит

- CLI commands;
- daemon-backed TUI;
- HTTP API;
- Telegram adapter.

## Ответственность

`Operator Surfaces` принимают input от оператора или внешнего транспорта и вызывают операции `App / Runtime Core`.

Этот container не должен содержать собственные:

- chat loop;
- prompt assembly;
- provider loop;
- tool execution semantics;
- schedule semantics;
- inter-agent routing.

## Основные связи

- `Operator` использует `Operator Surfaces`.
- `Operator Surfaces` вызывает `App / Runtime Core`.
- `Operator Surfaces` получает updates и отправляет notifications через `Telegram Bot API`.

## Правило изменения

Если для CLI, TUI, HTTP или Telegram нужна новая возможность, сначала добавляйте её в `App / Runtime Core`, а затем подключайте surface как thin adapter.
