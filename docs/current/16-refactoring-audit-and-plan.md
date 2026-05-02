# Аудит рефакторинга и план работ

Дата: 2026-05-02.

Статус: рабочий план. Цель документа — зафиксировать, где проект уже стал слишком тяжёлым для безопасных изменений, и разложить рефакторинг на небольшие этапы без изменения canonical runtime path.

## Инварианты

- Не заводить второй chat path, prompt path или отдельный tool loop для Telegram/TUI/CLI.
- Сохранять порядок prompt assembly: `SYSTEM.md`, `AGENTS.md`, `SessionHead`, `Plan`, `ContextSummary`, offload refs, uncovered transcript tail.
- Не возвращать shell-snippet style tools и скрытую shell-магию.
- Large outputs остаются через artifacts/offload, а не попадают обратно в prompt целиком.
- CLI, TUI, HTTP, Telegram и background jobs должны оставаться тонкими поверхностями над одним app/runtime слоем.

## Текущий срез

Workspace состоит из трёх Rust packages:

| Package | Назначение |
| --- | --- |
| `agent-runtime` | Типы runtime, prompt assembly, tool definitions/runtime, provider abstractions, план, память, skills, workspace. |
| `agent-persistence` | Конфигурация, SQLite schema, repositories, payload/artifact storage, audit. |
| `agentd` | App/bootstrap, execution loop, daemon, CLI, TUI, Telegram, HTTP, MCP, OTLP/trace. |

Baseline-команда:

```bash
CARGO_INCREMENTAL=0 cargo check --workspace --all-features
```

Результат на момент аудита: проходит.

Самые крупные production-файлы:

| Файл | Строк | Риск |
| --- | ---: | --- |
| `crates/agent-runtime/src/tool.rs` | ~7400 | Смешаны типы tools, catalog/schema, parsing, runtime execution, result rendering. Любое изменение tool contract задевает весь файл. |
| `cmd/agentd/src/tui.rs` | ~5900 | Центральная TUI state machine перегружена экранами, командами, навигацией и debug flows. |
| `cmd/agentd/src/execution/provider_loop.rs` | ~5000 | Смешаны cursor state, provider retries, prompt context, compaction, tool dispatch, ledger, offload, completion gate. Это самый критичный runtime path. |
| `cmd/agentd/src/telegram/router.rs` | ~3500 | Смешаны command parsing, pairing, session binding, inbound queue, file transfer, progress/status, delivery cursor. |
| `crates/agent-persistence/src/store/tests.rs` | ~3500 | Большой монолит тестов хранилища; сложно запускать точечно и понимать покрытие по repository. |
| `crates/agent-persistence/src/config.rs` | ~1750 | Конфиг вырос вместе с deployment/add-ons; риск случайных регрессий при изменении env/config loading. |

Самые тяжёлые тестовые зоны:

| Файл | Строк | Комментарий |
| --- | ---: | --- |
| `cmd/agentd/tests/bootstrap_app/context.rs` | ~4200 | Проверяет prompt/context/offload, но содержит много сценариев в одном файле. |
| `cmd/agentd/tests/bootstrap_app/chat.rs` | ~4000 | Главный integration safety net для canonical chat path. |
| `cmd/agentd/tests/telegram_surface.rs` | ~4000 | Хорошее покрытие Telegram, но файл перегружен разными сценариями. |
| `cmd/agentd/tests/tui_app.rs` | ~1900 | TUI debug/navigation/render flows смешаны. |

## Основные выводы аудита

### 1. Архитектура в целом правильная, но границы файлов запаздывают

Ключевой плюс проекта: runtime уже дисциплинирован вокруг одного canonical path. Поверхности не создают отдельную модель исполнения, а вызывают общий `App`/`ExecutionService`.

Проблема не в неправильной архитектуре, а в том, что несколько файлов стали контейнерами для целых подсистем. Это повышает стоимость каждого изменения и заставляет разработчика держать слишком много контекста одновременно.

### 2. `tool.rs` нужно разделять первым

`tool.rs` сейчас содержит:

- `ToolName`;
- input/output structs;
- `ToolCall`;
- `ToolOutput`;
- `ToolError`;
- `ToolCatalog`;
- OpenAI function schemas;
- parsing/repair;
- `ToolRuntime`;
- model-facing output rendering;
- web/fs/exec helper logic.

Это создаёт прямую причину ошибок вроде неправильного model-facing контракта `deliver_file`: внутренние details легко протекают в schema/output, потому что всё лежит рядом.

Целевое состояние: один публичный `agent_runtime::tool` module остаётся, но реализация раскладывается на подмодули по ответственности.

### 3. `provider_loop.rs` нельзя “переписать”, его надо расслаивать

Это наиболее рискованный файл. В нём проходит главный пользовательский turn. Рефакторинг должен быть mechanical и test-first:

- сначала вынести чистые helper-группы;
- сохранить публичные функции и тесты;
- не менять семантику retries, repeated tool-call guard, approvals, compaction, ledger и offload;
- после каждого слоя запускать targeted tests.

Любая попытка одновременно поменять поведение provider loop и разрезать файл будет опасной.

### 4. Telegram router уже стал отдельной подсистемой

В `telegram/router.rs` сейчас живут разные домены:

- operator commands;
- pairing/bindings;
- inbound queue/coalescing;
- progress/status message;
- files/documents;
- delivery cursor;
- rate limiting.

Эти домены можно разделить без изменения Telegram surface semantics. Это даст быстрый выигрыш для поддержки, потому что Telegram сейчас основной пользовательский surface.

### 5. TUI надо рефакторить после стабилизации runtime seams

TUI важен как debug UI, но сейчас риск рефакторинга TUI ниже, чем риск provider loop. Оптимальный порядок: сначала сделать runtime/tool/debug data более чистыми, потом облегчать TUI screens.

### 6. Legacy надо удалять только после явного compatibility решения

В docs и коде ещё видны legacy/compatibility paths:

- legacy filesystem tool ids (`fs_read`, `fs_write`, `fs_patch`, `fs_search`);
- legacy Obsidian/Logseq container paths;
- исторические deployment/docs sections.

Удалять их нужно не “по grep legacy”, а по правилу:

1. проверить, что они не входят в automatic model-facing surface;
2. проверить, что CLI/docs не рекомендуют их как основной путь;
3. оставить compatibility только там, где она реально нужна для старых sessions/configs;
4. удалить или пометить всё остальное.

## План работ

### P0. Зафиксировать safety baseline

Цель: перед рефакторингом иметь быстрые и полные проверки.

Работы:

- добавить или документировать короткий набор targeted commands для каждой подсистемы в [08-testing-and-verification.md](08-testing-and-verification.md);
- зафиксировать, какие тесты защищают canonical chat path, Telegram delivery, tool schema/output, persistence migrations;
- проверить, что `cargo check --workspace --all-features` проходит;
- перед крупными изменениями запускать full gate только на clean checkpoints.

Acceptance:

- есть документированный test matrix;
- каждый следующий refactor task указывает свой targeted gate;
- full verification остаётся стандартной перед release/deploy.

### P1. Разделить `agent-runtime::tool`

Цель: уменьшить риск изменений tool contract и сделать schemas/result rendering проверяемыми по семьям tools.

Предлагаемая структура:

```text
crates/agent-runtime/src/tool.rs          # public re-export facade
crates/agent-runtime/src/tool/
├── catalog.rs                            # ToolCatalog, ToolDefinition, ToolPolicy
├── names.rs                              # ToolName, families, automatic surface
├── inputs.rs                             # input structs/enums
├── outputs.rs                            # output structs/enums + summaries/model_output
├── parse.rs                              # ToolCall parsing/repair
├── runtime.rs                            # ToolRuntime and local fs/web/exec execution
├── schema.rs                             # provider function schemas
└── tests.rs                              # existing unit tests moved mechanically
```

Правила:

- public API для внешнего кода не ломать;
- first pass — только move/split, без изменения behavior;
- tests должны доказывать, что automatic model-facing surface не изменился.

Targeted checks:

```bash
CARGO_INCREMENTAL=0 cargo test -p agent-runtime tool
CARGO_INCREMENTAL=0 cargo test -p agent-runtime provider_contract
```

### P1. Расслаить `provider_loop.rs`

Цель: сделать главный execution loop читаемым без изменения runtime semantics.

Предлагаемая структура:

```text
cmd/agentd/src/execution/provider_loop.rs      # orchestration only
cmd/agentd/src/execution/provider_cursor.rs    # ProviderLoopCursor
cmd/agentd/src/execution/tool_ledger.rs        # tool_calls ledger + output artifacts
cmd/agentd/src/execution/tool_dispatch.rs      # execute_model_tool_call and helpers
cmd/agentd/src/execution/prompt_context.rs     # prompt_messages/session head/budget
cmd/agentd/src/execution/context_offload.rs    # offload read/search/pin/persist helpers
cmd/agentd/src/execution/completion_gate.rs    # completion nudge/gate decisions
```

Порядок:

1. вынести `ProviderLoopCursor` без изменения логики;
2. вынести ledger helpers;
3. вынести prompt context helpers;
4. вынести tool dispatch;
5. вынести offload helpers;
6. только после этого рассматривать behavior changes.

Targeted checks:

```bash
CARGO_INCREMENTAL=0 cargo test -p agentd provider_loop
CARGO_INCREMENTAL=0 cargo test -p agentd --test bootstrap_app chat
CARGO_INCREMENTAL=0 cargo test -p agentd --test bootstrap_app context
```

### P1. Разделить Telegram router

Цель: упростить развитие Telegram как основного surface.

Предлагаемая структура:

```text
cmd/agentd/src/telegram/router.rs       # high-level worker/router orchestration
cmd/agentd/src/telegram/commands.rs     # ParsedTelegramCommand, command registry/help
cmd/agentd/src/telegram/bindings.rs     # pairing, private/group bindings
cmd/agentd/src/telegram/queue.rs        # inbound queue/coalescing/queue commands
cmd/agentd/src/telegram/files.rs        # upload/download/file delivery helpers
cmd/agentd/src/telegram/progress.rs     # Working/Drafting status tracking
cmd/agentd/src/telegram/delivery.rs     # transcript delivery cursor/rate limiter
```

Правила:

- не менять Bot API behavior;
- не менять queue semantics;
- не менять file delivery queue contract;
- command parsing tests должны переехать вместе с `commands.rs`.

Targeted checks:

```bash
CARGO_INCREMENTAL=0 cargo test -p agentd --test telegram_surface
```

### P2. Разделить TUI state/screens/debug flows

Цель: сделать TUI пригодным для дальнейшего debug UI: traces, tools, artifacts, sessions.

Предлагаемая структура:

```text
cmd/agentd/src/tui.rs                  # app loop/bootstrap only
cmd/agentd/src/tui/state.rs            # TuiState and navigation state
cmd/agentd/src/tui/screens.rs          # screen enum and transitions
cmd/agentd/src/tui/commands.rs         # slash/backslash command parsing
cmd/agentd/src/tui/debug.rs            # session/tool/artifact debug browser state
cmd/agentd/src/tui/input.rs            # composer/input handling
```

Targeted checks:

```bash
CARGO_INCREMENTAL=0 cargo test -p agentd --test tui_app
CARGO_INCREMENTAL=0 cargo test -p agentd --test daemon_tui
```

### P2. Разделить persistence tests и schema helpers

Цель: снизить стоимость изменений SQLite schema/repositories.

Работы:

- перенести тесты из `store/tests.rs` по доменам: agents, sessions, runs/jobs, transcripts, artifacts, telegram, trace, migrations;
- выделить schema migration helpers по зонам, если это можно сделать без риска;
- добавить отдельные тесты на concurrency/busy retry рядом с store runtime tests.

Targeted checks:

```bash
CARGO_INCREMENTAL=0 cargo test -p agent-persistence store
CARGO_INCREMENTAL=0 cargo test -p agent-persistence config
```

### P2. Почистить legacy и документацию

Цель: уменьшить когнитивный шум для нового разработчика и оператора.

Работы:

- вынести legacy tool ids в отдельный compatibility section;
- убрать legacy Obsidian/Logseq из основных happy-path инструкций, оставить recovery/compat notes;
- синхронизировать `docs/current/10-tool-usability-assessment.md`, `11-workspace-modernization-plan.md`, `12-prompt-contract-decision.md`, `14-container-addons.md`, `15-tool-reference.md`;
- добавить короткий “runtime mental model” документ: `Operator -> Surface -> App -> Session -> Run -> ProviderLoop -> ToolCall -> Artifact/Delivery`.

Targeted checks:

```bash
rg -n "legacy|Obsidian|Logseq|fs_read\\b|fs_write\\b|fs_patch\\b|fs_search\\b" docs/current crates cmd
```

## Что не делать сейчас

- Не выделять новый crate для `agentd` internals до file-level split. Сначала нужны стабильные module seams.
- Не менять provider loop behavior одновременно с переносом кода.
- Не переписывать Telegram worker на новый framework.
- Не удалять compatibility tools без отдельной проверки старых sessions/configs.
- Не переносить persistence на другую БД.
- Не пытаться “почистить всё” одним PR/commit.

## Рекомендуемый порядок выполнения

1. P0 safety baseline.
2. P1 `agent-runtime::tool` split.
3. P1 `provider_loop.rs` cursor/ledger/prompt/offload split.
4. P1 Telegram router split.
5. P2 TUI debug/state split.
6. P2 persistence tests/schema cleanup.
7. P2 legacy/docs cleanup.

Причина порядка: сначала уменьшаем риск изменения model/tool contracts, затем главный execution loop, затем основной пользовательский surface Telegram, и только потом UI/debug/docs хвосты.
