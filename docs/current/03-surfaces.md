# Поверхности: CLI, daemon, HTTP API, TUI и Telegram

## Общий принцип

У системы несколько операторских поверхностей, но runtime один:

- CLI
- daemon
- HTTP API
- TUI
- Telegram

Они различаются только способом взаимодействия с оператором и транспортом, но не должны расходиться по semantics.

## CLI

Главный файл: [`cmd/agentd/src/cli.rs`](../../cmd/agentd/src/cli.rs)

CLI поддерживает две модели работы:

- локальный direct mode;
- daemon-backed mode.

Типичные команды:

- `status`
- `logs`
- `version`
- `update`
- `chat show/send/repl`
- `tui`
- `daemon`
- `mission tick`
- `approval approve`

### Почему CLI не всегда идёт напрямую в `App`

Некоторые команды специально выполняются через daemon transport, потому что:

- нужен единый background worker;
- нужен общий state для TUI и CLI;
- daemon умеет autospawn/restart compatibility handling.

## Daemon

Главный файл: [`cmd/agentd/src/daemon.rs`](../../cmd/agentd/src/daemon.rs)

Daemon — это долгоживущий процесс, который:

- поднимает tiny_http server;
- принимает HTTP requests;
- тикает background worker;
- работает с тем же `App`, что и локальный CLI.

Daemon нужен, чтобы TUI и другие процессы не исполняли runtime independently.

## HTTP server

Главный router: [`cmd/agentd/src/http/server.rs`](../../cmd/agentd/src/http/server.rs)

Сервер обслуживает маршруты:

- `/v1/status`
- `/v1/about`
- `/v1/diagnostics/tail`
- `/v1/update`
- `/v1/daemon/stop`
- `/v1/agents*`
- `/v1/agent-schedules*`
- `/v1/mcp/connectors*`
- `/v1/memory/*`
- `/v1/a2a/delegations*`
- `/v1/sessions*`
- `/v1/chat/turn`
- `/v1/chat/turn/stream`
- `/v1/runs/approve`
- `/v1/runs/approve/stream`

Важно: сервер не исполняет “особую daemon-логику”. Он просто маршрутизирует запрос в app/runtime слой.

## HTTP client и autospawn

Главный файл: [`cmd/agentd/src/http/client.rs`](../../cmd/agentd/src/http/client.rs)

Клиент умеет:

- пробовать подключиться к локальному daemon;
- проверять совместимость по `version`, `commit`, `tree_state`, `build_id`, `data_dir`;
- при необходимости автозапускать локальный daemon;
- перезапускать несовместимый локальный daemon;
- ждать его готовности poll’ами.

Это особенно важно для dirty builds: `commit=... tree=dirty` может быть одинаковым у разных локальных бинарей, поэтому введён `build_id`.

## TUI

Основные файлы:

- entrypoint: [`cmd/agentd/src/tui.rs`](../../cmd/agentd/src/tui.rs)
- state model: [`cmd/agentd/src/tui/app.rs`](../../cmd/agentd/src/tui/app.rs)
- backend contract: [`cmd/agentd/src/tui/backend.rs`](../../cmd/agentd/src/tui/backend.rs)
- worker: [`cmd/agentd/src/tui/worker.rs`](../../cmd/agentd/src/tui/worker.rs)
- rendering: [`cmd/agentd/src/tui/render.rs`](../../cmd/agentd/src/tui/render.rs)

### Что делает TUI

TUI отвечает за:

- отображение списков сессий;
- чат-экран;
- composer;
- dialogs/forms;
- browser’ы агентов, расписаний, MCP и artifacts;
- локальный UI state.

### Что TUI не должен делать

TUI не должен:

- иметь собственный execution loop;
- иметь отдельный prompt assembly;
- скрыто менять runtime semantics.

Поэтому у него есть `TuiBackend` trait, который тонко оборачивает app/daemon operations.

## TUI backend

`TuiBackend` в [`cmd/agentd/src/tui/backend.rs`](../../cmd/agentd/src/tui/backend.rs) описывает, что нужно интерфейсу:

- list/create/update sessions;
- читать transcript/approvals/skills;
- отправлять chat turns;
- approval continuation;
- работать с агентами, расписаниями, MCP;
- читать память, артефакты, debug bundle;
- отправлять agent messages и grants.

Важно: backend intentionally high-level. Он даёт TUI готовые операции, но не прячет отдельный runtime.

## Русские команды

TUI/REPL используют русскоязычную командную поверхность. Канонический help лежит в [`cmd/agentd/src/help.rs`](../../cmd/agentd/src/help.rs).

Примеры:

- `\судья <сообщение>`
- `\агент написать <id> <сообщение>`
- `\цепочка продолжить <chain-id> <причина>`
- `\память сессия <id> transcript`
- `\логи 100`

## Почему surface-тонкость важна

Если баг есть в provider loop, approval, persistence open path или inter-agent flow, его надо чинить в каноническом runtime, а не workaround’ом в TUI.

Именно поэтому недавние performance/runtime fixes шли через:

- `PersistenceStore::open_runtime(...)`
- `RuntimeTimingConfig`
- `session_transcript`/`pending_approvals` request paths
- daemon compatibility checks

а не через отдельные TUI hacks.

## Telegram

Основные файлы:

- entrypoint: [`cmd/agentd/src/telegram.rs`](../../cmd/agentd/src/telegram.rs)
- routing: [`cmd/agentd/src/telegram/router.rs`](../../cmd/agentd/src/telegram/router.rs)
- Bot API client: [`cmd/agentd/src/telegram/client.rs`](../../cmd/agentd/src/telegram/client.rs)
- rendering: [`cmd/agentd/src/telegram/render.rs`](../../cmd/agentd/src/telegram/render.rs)
- polling: [`cmd/agentd/src/telegram/polling.rs`](../../cmd/agentd/src/telegram/polling.rs)

Telegram запускается отдельной командой:

```bash
agentd telegram run
```

Этот процесс:

- получает updates через Telegram Bot API long polling;
- подключается к локальному daemon или autospawn-ит его;
- маршрутизирует обычные сообщения в canonical chat turn;
- отправляет replies, progress updates и reminders обратно в Telegram;
- хранит pairing records, chat bindings и update cursor в обычном runtime store.

Важно: Telegram не имеет отдельного prompt assembly, provider loop или tool loop. Это thin surface над тем же daemon/app/runtime path.

Практический setup описан в [telegram/01-install-and-configure.md](telegram/01-install-and-configure.md).
