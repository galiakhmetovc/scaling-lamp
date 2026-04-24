# Тесты и верификация

## Зачем здесь так много разных тестов

`teamD` — это не библиотека с одной функцией. Здесь баги обычно возникают на стыке:

- provider loop;
- SQLite request path;
- daemon client/server;
- TUI event flow;
- inter-agent chains;
- background jobs.

Поэтому здесь важны не только unit tests, но и integration/regression tests, которые проходят через несколько слоёв сразу.

## Основные группы тестов

### Runtime/persistence tests

- [`crates/agent-persistence/src/store/tests.rs`](../../crates/agent-persistence/src/store/tests.rs)
- [`crates/agent-runtime/src/tool/tests.rs`](../../crates/agent-runtime/src/tool/tests.rs)

Они проверяют:

- store semantics;
- locking-sensitive paths;
- config parsing;
- tool schemas и parsing;
- prompt/tool contract invariants.

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
sh scripts/test-deploy-teamd.sh
```

`test-deploy-teamd.sh` запускает `deploy-teamd.sh --help` и dry-run с fake secrets. Он не пишет в `/etc`, не вызывает real systemd start и нужен как быстрый guard для операторского install path.

Дополнительно smoke-test симулирует старый `cargo 1.75.0`, чтобы проверить bootstrap ветку: deploy script должен предложить установку/обновление stable Rust через `rustup`, потому что проект использует edition 2024.

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
