# agentd Clients

Связанный view: `SystemContext`.

Связанный C4-элемент: `agentd Clients`.

`agentd Clients` — способы взаимодействия оператора с execution mesh.

## Что входит

- CLI commands;
- TUI client;
- HTTP API clients;
- Telegram-mediated client flow.

## Важное правило

`agentd Clients` не исполняют агентскую работу.

Они отправляют команды, сообщения и запросы состояния в `teamD Execution Mesh`. Выполнение происходит на execution nodes внутри mesh.

## Почему Telegram не отдельный surface внутри mesh

Telegram — внешний транспорт и внешний API. Пользователь может общаться через Telegram client, но runtime state и agent turns всё равно обслуживаются execution mesh.
