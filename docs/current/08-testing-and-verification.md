# Тесты и верификация

## Зачем здесь так много разных тестов

`teamD` — это не библиотека с одной функцией. Здесь баги обычно возникают на стыке:

- provider loop;
- PostgreSQL request path;
- daemon client/server;
- TUI event flow;
- inter-agent chains;
- background jobs.

Поэтому здесь важны не только unit tests, но и integration/regression tests, которые проходят через несколько слоёв сразу.

## Основные группы тестов

### Runtime/persistence tests

- [`crates/agent-persistence/src/store/tests.rs`](../../crates/agent-persistence/src/store/tests.rs)
- [`crates/agent-persistence/src/store/tests/telegram.rs`](../../crates/agent-persistence/src/store/tests/telegram.rs)
- [`crates/agent-persistence/src/store/tests/tool_calls.rs`](../../crates/agent-persistence/src/store/tests/tool_calls.rs)
- [`crates/agent-persistence/src/store/tests/trace.rs`](../../crates/agent-persistence/src/store/tests/trace.rs)
- [`crates/agent-runtime/src/tool/tests.rs`](../../crates/agent-runtime/src/tool/tests.rs)

Они проверяют:

- store semantics;
- locking-sensitive paths;
- config parsing;
- tool schemas и parsing;
- prompt/tool contract invariants.

Store tests постепенно выносятся из одного большого файла в доменные модули. Цель — чтобы regression по Telegram state, tool-call ledger или traces можно было запускать и читать отдельно, не теряя общий `agent-persistence` gate.

### Bootstrap/app integration

- [`cmd/agentd/tests/bootstrap_app`](../../cmd/agentd/tests/bootstrap_app)

Тут проверяются execution scenarios через app/runtime:

- chat flow;
- inter-agent flow;
- memory/approval/integration cases.

### Daemon HTTP tests

- [`cmd/agentd/tests/daemon_http.rs`](../../cmd/agentd/tests/daemon_http.rs)

Проверяют server/client path и HTTP contract.

### TUI tests

- [`cmd/agentd/tests/tui_app.rs`](../../cmd/agentd/tests/tui_app.rs)
- [`cmd/agentd/tests/daemon_tui.rs`](../../cmd/agentd/tests/daemon_tui.rs)

Особенно важен `daemon_tui` слой, потому что он проверяет уже не просто local app state, а путь:

TUI -> daemon client -> daemon server -> runtime -> persistence

## Недавние важные regression areas

### Timing policy

Есть regression на central timing policy:

- [`cmd/agentd/tests/timing_policy.rs`](../../cmd/agentd/tests/timing_policy.rs)

### Inter-agent async follow-up

Есть tests на `message_agent` + `session_wait` semantics:

- [`cmd/agentd/tests/bootstrap_app/interagent.rs`](../../cmd/agentd/tests/bootstrap_app/interagent.rs)

### Daemon-backed TUI inter-agent flow

Есть regression, который:

- поднимает daemon-backed TUI path;
- отправляет сообщение Judge;
- ждёт появления child session;
- проверяет, что в дочернем transcript реально есть ответ.

Файл:

- [`cmd/agentd/tests/daemon_tui.rs`](../../cmd/agentd/tests/daemon_tui.rs)

Он ещё и пишет читаемый transcript dump в файл:

- `target/test-artifacts/daemon-tui-interagent-judge-chat.log`

## Канонические команды проверки

Если изменения meaningful, базовый набор такой:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build -p agentd
cargo build --release -p agentd
```

## Матрица targeted checks для рефакторинга

Перед структурным рефакторингом не нужно каждый раз запускать полный release gate на каждый маленький перенос кода. Но у каждого этапа должен быть свой быстрый guard, который покрывает конкретный seam.

| Зона изменения | Что защищает | Быстрая команда |
| --- | --- | --- |
| `agent-runtime::tool` | Tool names, schemas, parsing, model-facing output, local `ToolRuntime` behavior. | `CARGO_INCREMENTAL=0 cargo test -p agent-runtime tool` |
| Provider contract | Совместимость provider request/response abstractions и tool schema contract. | `CARGO_INCREMENTAL=0 cargo test -p agent-runtime --test provider_contract` |
| Provider loop internals | Repeated tool-call guard, recoverable tool errors, ledger/offload helpers, completion behavior. | `CARGO_INCREMENTAL=0 cargo test -p agentd provider_loop` |
| Canonical chat path | `App -> ExecutionService -> ProviderLoop -> transcript/run/tool ledger`. | `CARGO_INCREMENTAL=0 cargo test -p agentd --test bootstrap_app chat` |
| Prompt/context/offload | Prompt budget, compaction, context summary, offload refs/artifacts. | `CARGO_INCREMENTAL=0 cargo test -p agentd --test bootstrap_app context` |
| Telegram surface | Pairing, commands, queue/coalescing, status, files, delivery. | `CARGO_INCREMENTAL=0 cargo test -p agentd --test telegram_surface` |
| TUI local/debug | TUI state, render, debug browser, session/tool/artifact views. | `CARGO_INCREMENTAL=0 cargo test -p agentd --test tui_app` |
| TUI через daemon | `TUI -> daemon client -> HTTP daemon -> runtime` contract. | `CARGO_INCREMENTAL=0 cargo test -p agentd --test daemon_tui` |
| TUI module split smoke | Внутренние TUI helpers: browser items, debug bundle, command parsing, render/debug flows. | `CARGO_INCREMENTAL=0 cargo test -p agentd tui` |
| Persistence/schema | PostgreSQL schema, migration importer, repositories, payload/artifact storage, transaction/concurrency behavior. | `CARGO_INCREMENTAL=0 cargo test -p agent-persistence` |
| Persistence tool-call ledger | Запись arguments/result preview/artifact refs/status/error для tools. | `CARGO_INCREMENTAL=0 cargo test -p agent-persistence tool_calls` |
| Persistence Telegram state | Pairing, bindings, queue/status/cursor/file delivery records. | `CARGO_INCREMENTAL=0 cargo test -p agent-persistence telegram` |
| Persistence trace state | Trace links/spans для observability/debug. | `CARGO_INCREMENTAL=0 cargo test -p agent-persistence trace` |
| Config/deploy scripts | Config/env parsing and operator install/update scripts. | `CARGO_INCREMENTAL=0 cargo test -p agent-persistence config && sh scripts/test-deploy-teamd.sh` |
| Build baseline | Все crates компилируются со всеми features. | `CARGO_INCREMENTAL=0 cargo check --workspace --all-features` |

Эта матрица задаёт минимальную проверку для refactor tasks из [16-refactoring-audit-and-plan.md](16-refactoring-audit-and-plan.md). Если изменение пересекает несколько зон, запускайте объединение соответствующих команд.

## Full gate перед релизом или деплоем

Перед release/tag/deploy или крупным merge нужен полный набор:

```bash
cargo fmt --all
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo test --workspace --all-features
CARGO_INCREMENTAL=0 cargo build -p agentd
CARGO_INCREMENTAL=0 cargo build --release -p agentd
```

Если на машине мало места, сначала проверьте:

```bash
df -h .
du -sh target 2>/dev/null
```

Безопасная очистка build artifacts:

```bash
cargo clean
```

`cargo clean` удаляет только Cargo build output в `target/`; runtime state, workspace files и operator data не трогает.

## Точечные команды

### Только daemon-backed TUI inter-agent test

```bash
cargo test -p agentd --test daemon_tui daemon_backed_tui_can_send_judge_message_and_observe_child_reply -- --nocapture
```

### Прочитать transcript dump после этого теста

```bash
less target/test-artifacts/daemon-tui-interagent-judge-chat.log
```

### Проверить deploy script без systemd-изменений

```bash
sh -n scripts/deploy-teamd.sh
sh -n scripts/teamdctl.sh
sh -n scripts/deploy-teamd-binary.sh
sh scripts/test-deploy-teamd.sh
```

`test-deploy-teamd.sh` запускает `deploy-teamd.sh --help`, `deploy-teamd-binary.sh --help`, dry-run с fake secrets, binary deploy dry-run и smoke для `teamdctl telegram pair`. Он не пишет в `/etc`, не вызывает real systemd start и нужен как быстрый guard для операторского install path.

Дополнительно smoke-test симулирует старый `cargo 1.75.0`, чтобы проверить bootstrap ветку: deploy script должен предложить установку/обновление stable Rust через `rustup`, потому что проект использует edition 2024.

Ещё один smoke сценарий симулирует failing `pkg-config`, чтобы проверить bootstrap ветку системных build dependencies: deploy script должен предложить установку `pkg-config`, OpenSSL dev headers и C toolchain до `cargo build`.

## Что проверять руками

Автотесты важны, но для операторского UX полезно иногда прогонять и руками:

```bash
cargo build --release -p agentd
TEAMD_DATA_DIR=$(mktemp -d) target/release/agentd tui
```

Минимальный smoke:

1. открыть новую session;
2. отправить `\судья Кто ты?`;
3. вернуться к списку;
4. открыть `Agent: Judge`;
5. убедиться, что ответ реально пришёл.

## Что считать достаточной верификацией

Зависит от изменения:

- docs-only — перечитать изменённые документы и убедиться, что ссылки и команды не врут;
- config/runtime policy — unit + integration tests + targeted manual smoke;
- TUI/daemon/interagent — обязательно хотя бы один daemon-backed regression или live smoke.

Главный принцип: не говорить “работает”, пока не прогнаны реальные команды или тесты, которые это показывают.
