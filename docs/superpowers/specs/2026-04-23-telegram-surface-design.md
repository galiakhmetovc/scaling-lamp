# Telegram Surface Design

## Goal

Добавить в `agentd` новый операторский surface для Telegram, который:

- работает как отдельный процесс `agentd telegram run`;
- использует long polling;
- не вводит второй runtime path, второй prompt path или отдельный execution loop;
- позволяет работать с сессиями, командами, файлами, межагентным общением и фоновыми задачами через Telegram;
- использует pairing-модель доступа вместо открытого публичного бота.

Итоговая цель продукта: Telegram должен быть ещё одной тонкой поверхностью над тем же `App`/daemon/runtime, что и CLI/TUI/HTTP.

## Non-Goals

- Не встраивать Telegram polling внутрь `agentd daemon`.
- Не переносить runtime state в Telegram-specific dialogue framework.
- Не строить отдельный Telegram-specific tool loop или prompt assembly.
- Не ослаблять существующие security boundaries ради удобства Telegram UX.
- Не вводить “магический” синхронный inter-agent wait, который скрывает реальное состояние дочерней сессии.

## Product Requirements

Пользовательские требования для первого проекта:

- бот работает и в личных чатах, и в группах;
- доступ выдаётся через pairing:
  - пользователь пишет `/start`;
  - бот выдаёт одноразовый ключ;
  - оператор активирует ключ через CLI-команду;
- transport для старта: long polling;
- в личке доступны команды и обычный текст;
- в группе бот реагирует только если его упомянули, и только если отправитель уже активирован;
- в группе используется одна общая session на чат;
- Telegram должен уметь отправлять промежуточные статусы и финальные ответы, но не чаще одного статусного обновления в 30 секунд;
- нужно покрыть не только chat turn, но и inter-agent flow, auto-approve, files/artifacts;
- slash-команды должны быть английскими;
- команды бота должны регистрироваться в Telegram через Bot API;
- конфиг живёт в `config.toml`, секреты — в `.env`;
- Telegram surface запускается отдельной командой, а не как часть daemon lifecycle.

## Telegram Platform Constraints

Ниже перечислены ограничения, которые Telegram документирует официально для Bot API и которые нужно принять как реальные product/runtime constraints.

### 1. Message and Caption Sizes

- текст сообщения:
  - `1-4096` символов после entity parsing;
- редактируемый текст:
  - `1-4096` символов после entity parsing;
- caption у документов/медиа:
  - `0-1024` символов после entity parsing.

Следствие для дизайна:

- длинные runtime ответы нужно автоматически разбивать на сообщения или отправлять как файл/артефакт;
- длинные status/error dumps нельзя слать одним сообщением “как есть”;
- summaries для артефактов/файлов должны быть короткими, а полный контент — через файл или последовательность сообщений.

### 2. Bot Commands

- у одного набора команд максимум `100` команд;
- `BotCommand.command`:
  - `1-32` символа;
  - только строчные английские буквы, цифры и `_`;
- `BotCommand.description`:
  - `1-256` символов.

Следствие для дизайна:

- Telegram command registry должен публиковать только компактный операторский слой;
- нельзя механически выгрузить “весь help TUI” как список Telegram commands;
- длинные и редкие команды должны оставаться вызываемыми вручную текстом, даже если не все из них попадают в зарегистрированный список команд.

### 3. Long Polling

Для `getUpdates`:

- `limit` принимает значения `1-100`;
- положительный `timeout` используется для long polling;
- без offset подтверждаются только уже обработанные обновления, поэтому нужно durable хранение update cursor.

Следствие для дизайна:

- worker должен обрабатывать обновления батчами размером не больше 100;
- обязательно хранить последний подтверждённый `update_id`;
- нельзя рассчитывать на “бесконечный backlog за один запрос”.

### 4. File Transfer Limits on Official Cloud Bot API

Для обычного облачного Bot API:

- скачивание через `getFile` работает только для файлов до `20 MB`;
- multipart upload:
  - до `10 MB` для photos;
  - до `50 MB` для других файлов;
- отправка по URL:
  - до `5 MB` для photos;
  - до `20 MB` для других типов контента.

Следствие для дизайна:

- входящий файл больше `20 MB` нельзя считать поддержанным в базовом облачном сценарии, даже если бот получил update с metadata;
- безопасный baseline для первого релиза:
  - принимать входящие файлы только до `20 MB`;
  - отправлять исходящие документы только до `50 MB`;
- `telegram.max_upload_bytes` и `telegram.max_download_bytes` в конфиге должны валидироваться против этих ceiling’ов по умолчанию;
- если позже появится поддержка Local Bot API, эти лимиты можно будет расширять отдельно и явно.

### 5. Broadcast and Per-Chat Rate Limits

Из официального Bots FAQ:

- в одном чате не стоит отправлять больше `1 сообщения в секунду`;
- в группе бот не может отправлять больше `20 сообщений в минуту`;
- bulk notifications:
  - примерно `30 сообщений в секунду` бесплатно;
  - до `1000 сообщений в секунду` только через paid broadcasts, при выполнении условий Telegram.

Следствие для дизайна:

- текущий user requirement “не чаще раза в 30 секунд” для progress updates правильный и должен остаться hard limit;
- в группе нужно считать не только progress, но и стартовые/финальные/system сообщения;
- предпочтительная стратегия — редактировать одно status-message вместо отправки новых сообщений, когда это возможно;
- Telegram surface не должен делать chatty tool-by-tool narration.

### 6. Callback and Button Limits

- `InlineKeyboardButton.callback_data`:
  - `1-64` байта;
- callback query после нажатия кнопки должна завершаться вызовом `answerCallbackQuery`, иначе клиент показывает progress bar.

Следствие для дизайна:

- если позже появятся inline-кнопки для approvals/navigation, в callback payload нельзя запихивать большие структурированные данные;
- нужно использовать короткие opaque identifiers и хранить состояние на сервере;
- любой callback flow обязан быстро отвечать `answerCallbackQuery`, даже если основная работа идёт асинхронно.

### 7. What Telegram Does Not Document Precisely

Telegram не даёт официальной, полной таблицы лимитов для:

- частоты `editMessageText`;
- частоты `answerCallbackQuery`;
- сложных комбинаций send/edit/delete в одном чате;
- некоторых nuanced anti-spam heuristics.

Следствие для дизайна:

- опираться нужно только на официально документированные send/broadcast limits;
- для edit-heavy UX не делать агрессивных предположений;
- worker должен уметь получать `429` и откатываться через backoff/retry policy вместо жёстко прошитых “магических” предположений.

### 8. Operational Headroom Policy

Telegram surface не должен работать “впритык” к опубликованным потолкам. По умолчанию нужно использовать не более `80%` от официальных лимитов, чтобы оставить запас под:

- burstiness update processing;
- retries после transient ошибок;
- групповые anti-spam эвристики Telegram;
- сочетание send/edit/final/status/system сообщений в одном и том же чате.

Derived defaults для первого релиза:

- message text soft cap:
  - `3276` символов (`floor(4096 * 0.8)`)
- caption soft cap:
  - `819` символов (`floor(1024 * 0.8)`)
- incoming file soft cap:
  - `16 MB` (`floor(20 MB * 0.8)`)
- outgoing document soft cap:
  - `40 MB` (`floor(50 MB * 0.8)`)
- per-chat send budget:
  - `0.8 msg/sec`
  - practically: не больше `1` исходящего сообщения каждые `1250 ms`, если это не edit того же status-message
- per-group send budget:
  - `16 msg/min`
- global free broadcast budget:
  - `24 msg/sec`

Следствие для дизайна:

- `telegram.max_upload_bytes` и `telegram.max_download_bytes` в config должны по умолчанию выставляться на soft caps, а не на hard caps;
- Telegram renderer должен резать ответ по soft cap, а не ждать hard failure на 4096/1024;
- progress policy должна учитывать per-chat budget вместе с финальными и системными сообщениями;
- edit одного status-message предпочтительнее новых send-ов, потому что это снижает риск упереться в send-limit;
- retry/backoff policy должна считать `429` ожидаемой operational ситуацией, а не exceptional corruption case.

## Existing Baseline

В системе уже есть каноническая архитектура:

- `agent-runtime` — доменная модель runtime;
- `agent-persistence` — durable state/config/storage;
- `agentd` — CLI/daemon/TUI/execution integration.

Уже существуют:

- daemon-backed operator path;
- `DaemonClient` с autospawn/restart/compatibility logic;
- единый provider loop;
- единый execution orchestration path;
- session/agent/schedule/MCP/artifact/memory/inter-agent operations;
- `.env` + `config.toml` layered config model.

Это значит, что Telegram нужно добавлять не как новый runtime, а как ещё одну transport/UI поверхность.

## Core Design Decision

### 1. Telegram as a Thin Surface

Telegram добавляется как отдельный операторский surface:

- entrypoint: `agentd telegram run`
- transport: long polling
- execution: через тот же `App`/daemon-backed backend

Telegram не должен:

- собирать prompt;
- исполнять tool loop;
- принимать runtime-semantic decisions вместо канонического runtime.

### 2. Separate Process, Shared Runtime

Telegram worker запускается отдельным процессом, а не живёт внутри daemon.

Причины:

- проще изолировать polling lifecycle;
- проще перезапускать и дебажить;
- не утолщается daemon;
- сохраняется единый runtime path через уже существующий daemon/app layer.

### 3. Use `teloxide` as Transport SDK

Для интеграции с Telegram используется `teloxide`.

Он подходит как transport layer, потому что предоставляет:

- long polling;
- graceful shutdown;
- типизированный Telegram Bot API;
- `set_my_commands`;
- отправку файлов;
- скачивание файлов.

Но `teloxide` не должен становиться источником бизнес-логики системы.

Ограничение использования:

- использовать `teloxide` для polling, Telegram types, send/edit message, send/download file, command registration;
- не переносить state в `teloxide` dialogues;
- не строить всю систему вокруг `Dispatcher`/`dptree`;
- не заменять существующий command/router/runtime semantics на `BotCommands`-макрос как канонический parser.

## Configuration Model

### 1. Split Between `config.toml` and `.env`

Структурный конфиг должен жить в `config.toml`.

Секреты и overrides должны жить в `.env`.

### 2. New Config Section

В `AppConfig` добавляется секция `telegram`.

Минимальный набор полей:

- `enabled`
- `poll_interval_ms`
- `poll_request_timeout_seconds`
- `progress_update_interval_seconds`
- `pairing_token_ttl_seconds`
- `max_upload_bytes`
- `max_download_bytes`
- `private_chat_auto_create_session`
- `group_require_mention`
- `default_autoapprove`
- возможно `download_staging_dir` или derived staging policy

### 3. Environment Overrides

Секреты и критичные runtime overrides:

- `TEAMD_TELEGRAM_BOT_TOKEN`
- при необходимости дополнительные `TEAMD_TELEGRAM_*` overrides

Это должно использовать тот же layered config principle, который уже есть в репозитории.

## Access and Pairing Model

### 1. Unpaired Users

До pairing пользователь может получить только:

- одноразовый pairing token;
- подсказку `agentd telegram pair <key>`.

Все остальные команды и сообщения должны отклоняться.

### 2. Pairing Activation

Оператор активирует pairing token через CLI:

- `agentd telegram pair <key>`

Также нужны операторские команды управления:

- просмотр активных pairing records;
- revoke pairing.

### 3. Pairing Token Semantics

Pairing token должен быть:

- одноразовым;
- с TTL;
- durable;
- после активации становиться невалидным;
- пригодным для revoke/audit.

### 4. Group Access Rules

В группах бот должен реагировать только когда:

- его упомянули;
- отправитель уже активирован.

Наличие бота в группе не должно давать права неактивированным пользователям.

## Session and Chat Mapping

### 1. Private Chats

Для личного чата храним binding:

- `telegram_chat_id -> selected_session_id`

Обычный текст без команды:

- идёт в текущую выбранную session.

Если session ещё не выбрана:

- рекомендуется автоматически создать default session при первом нормальном сообщении после pairing.

### 2. Group Chats

Для группы используется один shared binding:

- `telegram_chat_id -> shared_session_id`

Обычный текст без mention:

- игнорируется.

Текст с mention:

- идёт в общую group-session.

### 3. Preferences

На уровень Telegram chat binding или chat-local state имеет смысл вынести:

- выбранную session;
- `autoapprove`;
- возможно selected agent / другие local overrides, если они понадобятся.

## Command Surface

### 1. Language

Slash-команды в Telegram — английские.

### 2. Parity Goal

Цель — полный operator parity с существующей системой.

Это не означает, что Telegram обязан копировать строку help из TUI один к одному. Это означает, что через Telegram можно добраться до тех же runtime capabilities.

### 3. Command Families

Командная поверхность должна покрывать:

- session/navigation:
  - `/start`, `/help`, `/new`, `/sessions`, `/use`, `/rename`
- chat/runtime:
  - обычный text turn
  - `/status`, `/jobs`, `/cancel`, `/stop`, `/pause`
- agent/inter-agent:
  - `/agents`, `/agent`, `/judge`, `/chain`, `/wait`
- artifacts/memory/files:
  - `/memory`, `/artifacts`, `/artifact`, file upload/download
- settings:
  - `/autoapprove`, `/approve`, `/model`, `/reasoning`, `/think`, `/compact`
- operator surfaces:
  - `/logs`, `/version`, `/mcp`, `/schedule`

### 4. Command Registration

Telegram worker должен вызывать Telegram Bot API `setMyCommands`, чтобы зарегистрировать актуальный набор slash-команд.

## Auto-Approve and Approval Semantics

В Telegram по умолчанию:

- `autoapprove = on`

Переключение должно быть доступно через slash-команду.

Это operator-level UX policy для Telegram surface, а не изменение общей execution semantics всей системы.

Manual approval UX в Telegram пока не должен становиться центральным способом работы. Telegram v1 должен оптимизироваться под auto-approve path.

## Inter-Agent Semantics

### 1. Honest Asynchrony

`message_agent` должен оставаться асинхронным.

Telegram surface не должен делать вид, что межагентный результат уже готов, если он ещё в работе.

### 2. Required Telegram UX

После межагентного действия бот должен уметь показать:

- `recipient_session_id`
- `recipient_job_id`
- `chain_id`
- текущий статус ожидания

Наблюдение за дочерней веткой должно идти через существующие runtime mechanics вроде `session_wait`, а не через скрытый special path.

## Progress and Message Rendering

### 1. Progress Policy

Для long-running turns Telegram worker должен:

- отправить стартовое подтверждение;
- обновлять статус не чаще чем раз в 30 секунд;
- отправить отдельный финальный ответ/ошибку.

### 2. Preferred Rendering Strategy

Рекомендуется:

- редактировать одно status-message во время выполнения;
- финал отправлять отдельным сообщением.

### 3. Error Rendering

Ошибки должны быть operator-readable:

- доступ запрещён до pairing;
- в группе нужен mention;
- session не выбрана;
- daemon/runtime недоступен;
- file слишком большой;
- команда использована с неверным форматом.

Бот не должен возвращать opaque internal errors без краткого пояснения.

## File Transport

### 1. Incoming Files

Telegram document/file upload должен:

- скачиваться из Telegram Bot API;
- сохраняться в контролируемую staging area;
- затем передаваться в канонический runtime path как локальный входной артефакт или файл.

Фото в первом приближении можно трактовать как обычный файл, без специального image-specific flow.

### 2. Outgoing Files

Если результат:

- короткий текст — отправляется сообщением;
- бинарник или большой offload — отправляется документом.

### 3. Limits and Safety

Нужны ограничения:

- по размеру входящих файлов;
- по размеру скачивания;
- по staging policy и очистке временных файлов.

Нельзя доверять имени файла как безопасному пути назначения.

## Persistence Model

Telegram state должен жить в `agent-persistence`, а не в отдельных JSON-файлах.

Новые persistent сущности:

- `TelegramPairingToken`
- `TelegramIdentity`
- `TelegramChatBinding`
- `TelegramUpdateCursor`
- при необходимости Telegram chat preferences

Это даст:

- durability;
- restart safety;
- единый recovery story;
- reuse существующих SQLite/WAL/busy-timeout guarantees.

## Architecture Components

Рекомендуемая структура в `cmd/agentd`:

- `src/telegram.rs`
  - entrypoint `agentd telegram run`
- `src/telegram/backend.rs`
  - thin backend over daemon/app operations
- `src/telegram/router.rs`
  - routing updates to commands/session actions
- `src/telegram/render.rs`
  - Telegram-specific rendering and rate-limited progress
- `src/telegram/pairing.rs`
  - pairing flow helpers
- возможно `src/telegram/files.rs`
  - file download/upload staging helpers

При этом канонический execution path остаётся в текущих `execution/*` и `App` APIs.

## End-to-End Flow

Упрощённый runtime flow:

1. `agentd telegram run` поднимает polling loop.
2. Telegram update приходит в router.
3. Router определяет:
   - тип чата;
   - pairing/access status;
   - mention/command/plain-text route;
   - chat/session binding.
4. Router вызывает backend operation.
5. Backend использует существующий daemon/app path для:
   - session ops;
   - chat turn;
   - agent message;
   - `session_wait`;
   - memory/artifact/file related ops.
6. Render слой превращает runtime response в Telegram output.

## Security Boundaries

Минимальные обязательные границы:

1. До pairing нет доступа к runtime commands.
2. В группах доступ только у активированного отправителя.
3. Pairing tokens bounded by TTL and one-time use.
4. Файлы скачиваются только в контролируемую staging area.
5. Telegram formatting должен экранироваться.
6. Никакого hidden privilege escalation через Telegram-specific shortcuts.

## Recovery and Failure Handling

После рестарта Telegram worker должен:

- продолжать polling с последнего `update_id`;
- не терять pairing/chat bindings;
- не ломать session mapping.

Если daemon временно недоступен:

- worker должен использовать существующий client compatibility/autospawn path;
- при невозможности восстановиться — вернуть пользователю понятный runtime-unavailable status;
- не уронить весь polling loop из-за одного failed update.

Каждый сбой Telegram worker должен попадать в существующий diagnostic pipeline.

## Testing Strategy

### 1. Unit Tests

- parsing slash-команд;
- mention parsing;
- pairing token lifecycle;
- chat binding resolution;
- progress throttling;
- Telegram text/file rendering.

### 2. Persistence and Config Tests

- загрузка `telegram` секции из `config.toml`;
- `.env` overrides;
- CRUD pairings/bindings/cursor;
- TTL/revoke semantics.

### 3. Integration Tests

- private chat -> session mapping;
- group mention -> shared session;
- inter-agent flow through Telegram surface;
- auto-approve behavior;
- file in/out path.

### 4. End-to-End Transport Simulation

Нужен fake Telegram API server, который позволяет проверять:

- `getUpdates`
- `sendMessage`
- `editMessageText`
- `sendDocument`
- `getFile`
- `setMyCommands`

Это позволит тестировать Telegram surface без реального внешнего бота.

## Delivery Strategy

Проект целевой большой, поэтому реализовывать его надо по вертикальным срезам.

### Recommended Phase 1

Первый рабочий срез:

- `agentd telegram run`
- `telegram` config + `.env` token
- `/start`
- pairing + `agentd telegram pair <key>`
- private chat bindings
- `/new`, `/sessions`, `/use`, `/help`
- обычный text -> canonical chat turn
- throttled progress + final response
- базовые config/persistence/integration/e2e tests

### Recommended Later Phases

Следующие срезы:

- groups + shared session semantics
- files/artifacts
- inter-agent + `session_wait`
- full command parity

## Acceptance Criteria

1. Telegram surface запускается отдельной командой `agentd telegram run`.
2. Telegram worker использует long polling и не вводит второй runtime path.
3. Pairing через `/start` + `agentd telegram pair <key>` работает durably.
4. Private chats могут отправлять обычный text в выбранную session.
5. Group chats работают только через mention и только для активированных пользователей.
6. Progress updates не спамят чаще установленного лимита.
7. Inter-agent and long-running statuses в Telegram отображаются честно, без скрытого fake wait.
8. Входящие и исходящие файлы проходят через безопасный staged path.
9. Команды бота регистрируются в Telegram.
10. Telegram state durable и переживает рестарт.
11. Integration/e2e tests покрывают transport-to-runtime flow.

## Source References

Официальные источники, на которые опирается этот design:

- Telegram Bot API:
  - https://core.telegram.org/bots/api
- Bots FAQ:
  - https://core.telegram.org/bots/faq
- Bot commands:
  - https://core.telegram.org/method/bots.setBotCommands
- Telegram command model:
  - https://core.telegram.org/api/bots/commands
- File transfer details:
  - https://core.telegram.org/bots/api#getfile
- Local Bot API capabilities:
  - https://core.telegram.org/bots/features#local-bot-api
