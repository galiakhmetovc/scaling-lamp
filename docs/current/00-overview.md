# Обзор системы

## Что делает `teamD`

`teamD` — это локальная среда для кодирующих агентов. Система хранит сессии, историю общения, планы, фоновые задачи, межагентные цепочки, артефакты и результаты работы инструментов. Оператор может работать с этим через CLI, HTTP API или полноэкранный TUI, но все три интерфейса должны вести в один и тот же runtime.

Проще говоря:

- оператор открывает сессию;
- пишет сообщение агенту;
- система собирает prompt из системных блоков и истории;
- вызывает provider;
- provider может попросить инструменты;
- runtime исполняет инструменты, approval’ы и фоновые jobs;
- результаты сохраняются в SQLite и связанных payload-файлах;
- CLI/TUI/HTTP просто показывают одно и то же состояние разными способами.

## Главные сущности

### App

[`App`](../../cmd/agentd/src/bootstrap.rs) — корневой объект процесса. Он знает:

- какой конфиг загружен;
- где лежат persistent stores;
- где workspace;
- как собран runtime scaffold;
- как ходить к release updater, processes registry и MCP registry.

Практически всё операторское API начинается с `bootstrap::build()`, которое создаёт `App`, а потом `App::run()` передаёт управление CLI/daemon/TUI.

### Session

`Session` — один диалог агента. У него есть:

- `id`
- `title`
- `agent_profile_id`
- `settings`
- timestamps
- optional parent/delegation metadata

Сессия — это контейнер для transcript, runs, jobs, планов, approvals, memory и inter-agent chain state.

### Run

`Run` — конкретный ход модели. Обычно это:

- обычный chat turn;
- background chat turn;
- approval continuation;
- wakeup turn.

У run есть `status`, `recent_steps`, provider usage, pending approvals, loop state и итоговый текст/ошибка.

### Job

`Job` — рабочая единица вокруг run. Нужна, потому что не всё исполняется синхронно в том же потоке, где был создан пользовательский запрос.

Примеры job kinds:

- `ChatTurn`
- `ScheduledChatTurn`
- `MissionTurn`
- `InterAgentMessage`
- `ApprovalContinuation`
- `Delegate`

Фоновые worker’ы гоняют именно jobs, а не “сырые” transcript-записи.

### Tool

`Tool` — структурированный вызов capability, который модель делает через канонический tool surface. Определения живут в [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs).

Главная идея: модель не должна изобретать shell snippets. Она вызывает named tool с typed input, а runtime сам решает, как это выполнить и как отдать результат обратно.

### Artifact

Большие tool outputs не пихаются целиком в prompt. Они уходят в artifact/offload storage, а модель получает bounded summary и ссылку на артефакт.

### Agent profile

Agent profile — это персонализация агента:

- имя;
- шаблон (`default`, `judge`);
- `SYSTEM.md` и `AGENTS.md` в `agent_home`;
- allowlist capabilities;
- operator-visible metadata.

## Как один запрос проходит через систему

Упрощённая цепочка:

1. Оператор пишет сообщение в CLI/TUI или вызывает HTTP endpoint.
2. Поверхность обращается к `App`.
3. `App` создаёт `ExecutionService`.
4. `ExecutionService` загружает session, transcripts, runs, plan, context summary, skills.
5. `prompting.rs` собирает `SessionHead`.
6. `PromptAssembly` строит messages в каноническом порядке.
7. Provider получает request и может вернуть текст, reasoning и/или tool calls.
8. `provider_loop.rs` выполняет tool round’ы, approvals и continuation logic.
9. Все изменения пишутся в `PersistenceStore`.
10. CLI/TUI/HTTP читают обновлённое состояние из store.

## Почему daemon-centered

В проекте специально избегают отдельного “TUI-runtime” или “CLI-runtime”. Идея такая:

- execution semantics должны быть одинаковыми;
- баги и recovery должны чиниться один раз, а не по слоям;
- transcript, approvals, jobs и inter-agent state должны быть общими;
- тесты должны проверять один execution path.

Это видно по тому, что:

- CLI умеет работать напрямую или через daemon;
- TUI использует daemon-backed backend;
- HTTP endpoints и TUI backend вызывают те же операции `App` и `ExecutionService`.

## Куда идти дальше

- Общая архитектура: [01-architecture.md](01-architecture.md)
- Prompt и chat turn: [02-prompt-and-turn-flow.md](02-prompt-and-turn-flow.md)
- Интерфейсы CLI/HTTP/TUI: [03-surfaces.md](03-surfaces.md)
- Хранилище и recovery: [06-storage-recovery-and-diagnostics.md](06-storage-recovery-and-diagnostics.md)
