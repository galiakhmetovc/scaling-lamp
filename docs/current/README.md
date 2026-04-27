# Текущая документация `teamD`

Этот каталог описывает текущую реализацию системы. Цель — дать начинающему разработчику и оператору понятную карту того, что делает runtime, из каких слоёв он состоит и в каких файлах искать детали.

## Как читать

Если вы впервые видите проект:

1. [00-overview.md](00-overview.md)
2. [01-architecture.md](01-architecture.md)
3. [02-prompt-and-turn-flow.md](02-prompt-and-turn-flow.md)
4. [03-surfaces.md](03-surfaces.md)
5. [06-storage-recovery-and-diagnostics.md](06-storage-recovery-and-diagnostics.md)

Если вы оператор:

1. [09-operator-cheatsheet.md](09-operator-cheatsheet.md)
2. [telegram/01-install-and-configure.md](telegram/01-install-and-configure.md)
3. [07-config.md](07-config.md)
4. [06-storage-recovery-and-diagnostics.md](06-storage-recovery-and-diagnostics.md)

Если вы разработчик, который будет менять runtime:

1. [00-overview.md](00-overview.md)
2. [01-architecture.md](01-architecture.md)
3. [02-prompt-and-turn-flow.md](02-prompt-and-turn-flow.md)
4. [04-tools-and-approvals.md](04-tools-and-approvals.md)
5. [05-interagent-background-and-schedules.md](05-interagent-background-and-schedules.md)
6. [08-testing-and-verification.md](08-testing-and-verification.md)
7. [10-tool-usability-assessment.md](10-tool-usability-assessment.md)
8. [11-workspace-modernization-plan.md](11-workspace-modernization-plan.md)
9. [12-prompt-contract-decision.md](12-prompt-contract-decision.md)
10. [13-observability-tracing-plan.md](13-observability-tracing-plan.md)
11. [14-container-addons.md](14-container-addons.md)
12. [15-tool-reference.md](15-tool-reference.md)

## Словарь

- **daemon** — долгоживущий HTTP-процесс, который обслуживает CLI и TUI.
- **session** — один диалог/контекст агента.
- **run** — одно выполнение модели внутри session.
- **job** — фоновая или сервисная работа вокруг run: chat turn, approval continuation, inter-agent message, schedule delivery, delegate job.
- **tool** — структурированный capability call модели.
- **artifact** — большой offloaded payload, который не кладут напрямую в prompt.
- **tool-call ledger** — журнал фактов вызова tools: имя, arguments, статус, ошибка, run/session.
- **agent home** — каталог prompts/skills конкретного agent profile; это не project workspace.
- **workspace** — рабочий каталог проекта, где tools читают/пишут файлы и запускают команды.
- **context summary / compaction** — сжатие истории сессии в summary.
- **SessionHead** — сжатая сводка о состоянии сессии, которую модель получает перед transcript tail.
- **Prompt contract** — договорённость о том, какие runtime/user/history blocks модель получает, в каком порядке и с какими ограничениями размера.
- **Trace / span** — локальная observability-модель для причинной связи между surface event, run, provider round, transcript, tool call, artifact и delivery; см. [13-observability-tracing-plan.md](13-observability-tracing-plan.md).
- **Container add-ons** — внешняя обвязка вокруг host `agentd`: Docker, SearXNG, Obsidian, Jaeger, Caddy; см. [14-container-addons.md](14-container-addons.md).

## Где искать код

- Точки входа: [`cmd/agentd/src/main.rs`](../../cmd/agentd/src/main.rs), [`cmd/agentd/src/bootstrap.rs`](../../cmd/agentd/src/bootstrap.rs)
- Prompt assembly: [`cmd/agentd/src/prompting.rs`](../../cmd/agentd/src/prompting.rs), [`crates/agent-runtime/src/prompt.rs`](../../crates/agent-runtime/src/prompt.rs)
- Execution: [`cmd/agentd/src/execution.rs`](../../cmd/agentd/src/execution.rs) и подпапка [`cmd/agentd/src/execution`](../../cmd/agentd/src/execution)
- Persistence: [`crates/agent-persistence/src/store.rs`](../../crates/agent-persistence/src/store.rs)
- Tools: [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs)
- CLI: [`cmd/agentd/src/cli.rs`](../../cmd/agentd/src/cli.rs)
- HTTP: [`cmd/agentd/src/http/server.rs`](../../cmd/agentd/src/http/server.rs), [`cmd/agentd/src/http/client.rs`](../../cmd/agentd/src/http/client.rs)
- TUI: [`cmd/agentd/src/tui.rs`](../../cmd/agentd/src/tui.rs), [`cmd/agentd/src/tui/app.rs`](../../cmd/agentd/src/tui/app.rs), [`cmd/agentd/src/tui/backend.rs`](../../cmd/agentd/src/tui/backend.rs)
- Telegram: [`cmd/agentd/src/telegram.rs`](../../cmd/agentd/src/telegram.rs), [`cmd/agentd/src/telegram/router.rs`](../../cmd/agentd/src/telegram/router.rs), [`cmd/agentd/src/telegram/client.rs`](../../cmd/agentd/src/telegram/client.rs)

## Архитектурные диаграммы

C4-модель хранится отдельно в [`docs/architecture`](../architecture). Источник истины для диаграмм — [`workspace.dsl`](../architecture/workspace.dsl), а команды локального просмотра описаны в [`docs/architecture/README.md`](../architecture/README.md).

## Что важно помнить

- Эта документация описывает **текущее состояние**, а не целевую архитектуру “когда-нибудь”.
- Исторические design notes лежат в [`docs/superpowers`](../superpowers), но они не заменяют описание текущего поведения.
- Если факты в документации и коде расходятся, источником истины остаётся код. В документах ниже даны ссылки на реальные файлы, чтобы быстро перепроверять детали.
