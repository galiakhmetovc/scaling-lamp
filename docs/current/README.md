# Текущая документация `teamD`

Этот каталог описывает текущую реализацию системы. Цель — дать начинающему разработчику и оператору понятную карту того, что делает runtime, из каких слоёв он состоит и в каких файлах искать детали.

## Как читать

Если вы впервые видите проект:

1. [00-overview.md](00-overview.md)
2. [17-runtime-mental-model.md](17-runtime-mental-model.md)
3. [01-architecture.md](01-architecture.md)
4. [02-prompt-and-turn-flow.md](02-prompt-and-turn-flow.md)
5. [03-surfaces.md](03-surfaces.md)
6. [06-storage-recovery-and-diagnostics.md](06-storage-recovery-and-diagnostics.md)

Если вы оператор:

1. [09-operator-cheatsheet.md](09-operator-cheatsheet.md)
2. [telegram/01-install-and-configure.md](telegram/01-install-and-configure.md)
3. [07-config.md](07-config.md)
4. [06-storage-recovery-and-diagnostics.md](06-storage-recovery-and-diagnostics.md)

Если вы разработчик, который будет менять runtime:

1. [00-overview.md](00-overview.md)
2. [17-runtime-mental-model.md](17-runtime-mental-model.md)
3. [01-architecture.md](01-architecture.md)
4. [02-prompt-and-turn-flow.md](02-prompt-and-turn-flow.md)
5. [04-tools-and-approvals.md](04-tools-and-approvals.md)
6. [05-interagent-background-and-schedules.md](05-interagent-background-and-schedules.md)
7. [08-testing-and-verification.md](08-testing-and-verification.md)
8. [10-tool-usability-assessment.md](10-tool-usability-assessment.md)
9. [11-workspace-modernization-plan.md](11-workspace-modernization-plan.md)
10. [12-prompt-contract-decision.md](12-prompt-contract-decision.md)
11. [13-observability-tracing-plan.md](13-observability-tracing-plan.md)
12. [14-container-addons.md](14-container-addons.md)
13. [15-tool-reference.md](15-tool-reference.md)
14. [16-refactoring-audit-and-plan.md](16-refactoring-audit-and-plan.md)

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
- **Container add-ons** — внешняя обвязка вокруг host `agentd`: Docker, SearXNG, SilverBullet, SilverBullet MCP, Browserless, Mem0, Jaeger, Caddy и legacy Obsidian; см. [14-container-addons.md](14-container-addons.md).
- **Runtime mental model** — сквозная цепочка `Operator -> Surface -> App -> Session -> Run -> ProviderLoop -> ToolCall -> Artifact/Delivery`; см. [17-runtime-mental-model.md](17-runtime-mental-model.md).

## Где искать код

- Точки входа: [`cmd/agentd/src/main.rs`](../../cmd/agentd/src/main.rs), [`cmd/agentd/src/bootstrap.rs`](../../cmd/agentd/src/bootstrap.rs)
- Prompt assembly: [`cmd/agentd/src/prompting.rs`](../../cmd/agentd/src/prompting.rs), [`crates/agent-runtime/src/prompt.rs`](../../crates/agent-runtime/src/prompt.rs)
- Execution: [`cmd/agentd/src/execution.rs`](../../cmd/agentd/src/execution.rs) и подпапка [`cmd/agentd/src/execution`](../../cmd/agentd/src/execution); provider loop helpers лежат рядом как `provider_cursor`, `provider_ledger`, `provider_prompt`, `provider_offload`, `provider_tool_dispatch`, `provider_completion`
- Persistence: [`crates/agent-persistence/src/store.rs`](../../crates/agent-persistence/src/store.rs)
- Tools: [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs)
- CLI: [`cmd/agentd/src/cli.rs`](../../cmd/agentd/src/cli.rs)
- HTTP: [`cmd/agentd/src/http/server.rs`](../../cmd/agentd/src/http/server.rs), [`cmd/agentd/src/http/client.rs`](../../cmd/agentd/src/http/client.rs)
- TUI: [`cmd/agentd/src/tui.rs`](../../cmd/agentd/src/tui.rs), [`cmd/agentd/src/tui/app.rs`](../../cmd/agentd/src/tui/app.rs), [`cmd/agentd/src/tui/backend.rs`](../../cmd/agentd/src/tui/backend.rs), debug/browser helpers в [`cmd/agentd/src/tui`](../../cmd/agentd/src/tui)
- Telegram: [`cmd/agentd/src/telegram.rs`](../../cmd/agentd/src/telegram.rs), [`cmd/agentd/src/telegram/router.rs`](../../cmd/agentd/src/telegram/router.rs), [`cmd/agentd/src/telegram/client.rs`](../../cmd/agentd/src/telegram/client.rs), доменные helpers в [`cmd/agentd/src/telegram`](../../cmd/agentd/src/telegram)

## Архитектурные диаграммы

C4-модель хранится отдельно в [`docs/architecture`](../architecture). Источник истины для диаграмм — [`workspace.dsl`](../architecture/workspace.dsl), а команды локального просмотра описаны в [`docs/architecture/README.md`](../architecture/README.md).

## Что важно помнить

- Эта документация описывает **текущее состояние**, а не целевую архитектуру “когда-нибудь”.
- Исторические design notes лежат в [`docs/superpowers`](../superpowers), но они не заменяют описание текущего поведения.
- Если факты в документации и коде расходятся, источником истины остаётся код. В документах ниже даны ссылки на реальные файлы, чтобы быстро перепроверять детали.
