# teamD Agent Runtime

`teamD` — локальная среда для автономных кодирующих агентов. В текущем состоянии это не набор разрозненных скриптов, а один daemon-centered runtime с общей моделью данных, общим execution path и несколькими операторскими поверхностями поверх него: CLI, HTTP API и TUI.

Главная идея проекта: любой ход агента, approval, фоновая задача, межагентное сообщение, MCP-вызов или wakeup должны проходить через один и тот же runtime, а не через отдельные “особые режимы” для разных интерфейсов.

## Что лежит в репозитории

- [`cmd/agentd`](cmd/agentd) — бинарь `agentd`, daemon, CLI, HTTP API, TUI, bootstrap и orchestration.
- [`crates/agent-runtime`](crates/agent-runtime) — runtime-модель: provider contracts, prompt assembly, tools, sessions, plans, runs, permissions, inter-agent primitives.
- [`crates/agent-persistence`](crates/agent-persistence) — конфиг, SQLite-хранилище, артефакты, recovery, audit и репозитории доступа к данным.

## Быстрый старт

Сборка:

```bash
cargo build -p agentd
cargo build --release -p agentd
```

Быстрые проверки:

```bash
cargo run -p agentd -- version
cargo run -p agentd -- status
cargo run -p agentd -- tui
```

Если нужен конфиг, возьмите за основу [config.example.toml](config.example.toml) и положите копию в `~/.config/teamd/config.toml` или укажите путь через `TEAMD_CONFIG`.

## Быстрый deploy Telegram/systemd

Из checkout можно развернуть production-like локальный runtime скриптом:

```bash
./scripts/deploy-teamd.sh
```

Он проверит `cargo`/`rustc`, при необходимости поставит или обновит stable Rust через `rustup`, интерактивно спросит Telegram bot token и Z.ai/API key, соберёт release binary, установит `agentd` в `/opt/teamd/bin`, создаст `/etc/teamd/config.toml`, `/etc/teamd/teamd.env` и systemd services `teamd-daemon.service`/`teamd-telegram.service`.

Подробности: [docs/current/telegram/01-install-and-configure.md](docs/current/telegram/01-install-and-configure.md).

## Каноническая документация по текущему состоянию

Эта документация описывает не vision и не roadmap, а то, как система устроена сейчас.

- [docs/current/README.md](docs/current/README.md) — карта документации и рекомендуемый порядок чтения.
- [docs/current/00-overview.md](docs/current/00-overview.md) — обзор системы простыми словами.
- [docs/current/01-architecture.md](docs/current/01-architecture.md) — слои, ключевые модули и общий data flow.
- [docs/current/02-prompt-and-turn-flow.md](docs/current/02-prompt-and-turn-flow.md) — prompt assembly, provider loop, tool loop, approvals и compaction.
- [docs/current/03-surfaces.md](docs/current/03-surfaces.md) — CLI, daemon, HTTP API и TUI как тонкие клиенты одного runtime.
- [docs/current/04-tools-and-approvals.md](docs/current/04-tools-and-approvals.md) — structured tool surface, approval semantics и частые ошибки использования.
- [docs/current/05-interagent-background-and-schedules.md](docs/current/05-interagent-background-and-schedules.md) — judge, межагентные цепочки, background jobs, wakeups и расписания.
- [docs/current/06-storage-recovery-and-diagnostics.md](docs/current/06-storage-recovery-and-diagnostics.md) — SQLite/store layout, recovery, diagnostics и отладка.
- [docs/current/07-config.md](docs/current/07-config.md) — `config.toml`, env overrides, timing/limits и provider settings.
- [docs/current/08-testing-and-verification.md](docs/current/08-testing-and-verification.md) — как проверяется система и какие regression tests уже есть.
- [docs/current/09-operator-cheatsheet.md](docs/current/09-operator-cheatsheet.md) — практические команды и сценарии для оператора.

Исторические design- и planning-документы остаются в [`docs/superpowers`](docs/superpowers), но это не каноническая документация текущей реализации.

## Базовые инварианты

- Один канонический runtime path: CLI, HTTP и TUI не должны иметь отдельный execution loop.
- Порядок prompt assembly фиксирован:
  1. `SYSTEM.md`
  2. `AGENTS.md`
  3. `SessionHead`
  4. `Plan`
  5. `ContextSummary`
  6. offload refs
  7. uncovered transcript tail
- Structured tools важнее shell-магии.
- Большие tool outputs должны уходить в artifacts/offload, а не молча в prompt.
- TUI и CLI должны оставаться thin clients над тем же app/runtime слоем.

Эти инварианты отражены в [AGENTS.md](AGENTS.md), [`cmd/agentd/src/prompting.rs`](cmd/agentd/src/prompting.rs), [`crates/agent-runtime/src/prompt.rs`](crates/agent-runtime/src/prompt.rs) и [`crates/agent-runtime/src/tool.rs`](crates/agent-runtime/src/tool.rs).

## Операторские команды

Минимальный набор:

```bash
cargo run -p agentd -- version
cargo run -p agentd -- status
cargo run -p agentd -- logs 100
cargo run -p agentd -- tui
cargo run -p agentd -- daemon
cargo run -p agentd -- daemon stop
```

Через TUI/REPL основные русские команды описаны в [`cmd/agentd/src/help.rs`](cmd/agentd/src/help.rs) и собраны в [docs/current/09-operator-cheatsheet.md](docs/current/09-operator-cheatsheet.md).

## Проверка изменений

Базовый набор команд для meaningful changes:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build -p agentd
cargo build --release -p agentd
```

## Лицензия и репозиторий

- Лицензия: MIT
- Репозиторий: <https://github.com/galiakhmetovc/scaling-lamp>
