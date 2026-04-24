# Telegram: установка и эксплуатация

Этот раздел описывает практический путь для оператора, который работает с `teamD` через Telegram.

## Документы

1. [01-install-and-configure.md](01-install-and-configure.md) — установка из репозитория, сборка `agentd`, `config.toml`, `.env`, запуск Telegram worker и pairing.
2. [02-file-transfer-plan.md](02-file-transfer-plan.md) — черновик приёма и отправки файлов через Telegram: UX, artifacts, commands, limits и безопасность.

## Связанные архитектурные материалы

- [TelegramDeployment](../../architecture/02-telegram-deployment.md) — deployment view: `Telegram Client` -> `Telegram Bot API` -> `agentd` -> local state/resources и `LLM Provider`.
- [Глоссарий](../../architecture/docs/03-terms.md) — термины `Telegram-mediated client flow`, `TelegramDeployment`, `Execution Node`, `agentd instance`.

## Короткая модель

Telegram-интеграция не создаёт отдельный runtime. Процесс `agentd telegram run` работает как клиент/worker поверх того же daemon-backed runtime:

- получает updates из `Telegram Bot API` через long polling;
- подключается к локальному daemon или autospawn-ит его;
- отправляет сообщения в canonical chat/runtime path;
- доставляет replies, progress updates и reminders обратно в Telegram;
- хранит pairing, bindings, cursors и sessions в обычном `data_dir`.
