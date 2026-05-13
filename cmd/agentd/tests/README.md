# agentd integration test suites

Эта папка держит integration tests по runtime-доменам. Новые проверки нужно класть в ближайший доменный suite, а не добавлять в самый большой файл по инерции.

## Домены

- `bootstrap_app/` — App/bootstrap операции без HTTP/TUI/Telegram транспорта.
- `daemon_http.rs` — daemon HTTP API и remote A2A endpoints. Общие fixtures вынесены в `daemon_http/support.rs`.
- `telegram_surface.rs` — Telegram Bot API surface, command routing, delivery, file flow and formatting. Новые крупные группы тестов нужно выносить в отдельный `telegram_*` suite или подмодуль.
- `tui_app.rs`, `daemon_tui.rs`, `skills_tui.rs` — TUI behavior and daemon-backed TUI paths.
- `nats_event_bus.rs`, `router_worker.rs`, `session_event_worker.rs`, `delivery_event_worker.rs`, `event_runtime_*` — event bus, NATS/router/session/delivery worker behavior.
- `session_context.rs` — prompt/context assembly and session memory behavior.
- `tool_call_smoke.rs`, `chat_smoke.rs`, `autonomous_smoke.rs` — bounded smoke checks over canonical runtime paths.

## Правила добавления тестов

- Если тест требует daemon HTTP fixture, используй `daemon_http/support.rs`.
- Если проверяется чистая App operation, добавляй в `bootstrap_app/<domain>.rs`.
- Если тест проверяет только parser/render/helper без network runtime, предпочитай unit test в соответствующем crate/module.
- Если тест становится длиннее сценария из 1-2 действий, сначала вынеси fixture/builder, потом добавляй сценарий.
- Не добавляй новый “god test file”; при появлении нового surface заводи отдельный suite.
