# Хранилище, recovery и диагностика

## Store layout

Главный storage слой — [`PersistenceStore`](../../crates/agent-persistence/src/store.rs).

По умолчанию layout строится от `data_dir` и содержит:

- `state.sqlite` — метаданные
- `artifacts/` — бинарные payload’ы
- `archives/` — архивы сессий
- `runs/` — run-related payload storage
- `transcripts/` — transcript payload storage

То есть состояние — это не “только SQLite”. SQLite хранит метаданные и индексы, а большие тела лежат рядом на файловой системе.

## Два режима открытия store

Это один из самых важных недавних runtime-fix’ов.

### `PersistenceStore::open(...)`

Используется для bootstrap path:

- готовит layout;
- открывает SQLite;
- настраивает соединение;
- bootstrap’ит и валидирует schema;
- делает reconcile orphan payloads.

### `PersistenceStore::open_runtime(...)`

Используется в request path:

- открывает уже готовую БД;
- не делает тяжёлую bootstrap/reconcile work;
- нужен для быстрых HTTP/TUI reads.

Это разделение устраняет зависания вроде “открытие transcript-tail внезапно 10 секунд думает”.

## SQLite runtime политика

Сейчас для SQLite явно настроены:

- `WAL` для лучшего read/write поведения;
- `busy_timeout`, который читается из `runtime_timing.sqlite_busy_timeout_ms`.

Это уменьшает риск `database is locked` в сценариях, где TUI/daemon/request-path пересекаются с writer lock.

## Почему store больше не должен тормозить TUI

Проблемы последних раундов были как раз тут:

- request-path раньше иногда делал слишком тяжёлую bootstrap work;
- чтение approvals/transcript могло упираться в write lock;
- summary/status routes раньше могли сканировать глобальное состояние тяжелее, чем нужно.

Сейчас архитектурная цель такая:

- bootstrap work только на bootstrap path;
- request-path читает только нужный scoped state;
- локальные session views не зависят от чужих тяжёлых runs.

## Recovery

Файл: [`crates/agent-persistence/src/recovery.rs`](../../crates/agent-persistence/src/recovery.rs)

На старте daemon/runtime выполняется recovery pass.

### Политика по умолчанию

`RecoveryMode::Reconcile` делает консервативное поведение:

- `running`
- `waiting_process`
- `waiting_delegate`
- `resuming`

могут быть переведены в `interrupted`, если у run нет recoverable active job.

Это означает: система предпочитает честно пометить “этот run оборвался при рестарте”, а не делать вид, что она точно умеет возобновить всё.

## Diagnostics и audit log

Файл: [`cmd/agentd/src/diagnostics.rs`](../../cmd/agentd/src/diagnostics.rs)

Диагностика пишется structured JSON events в:

- `data_dir/audit/runtime.jsonl`

Через `DiagnosticEventBuilder` события получают:

- timestamp;
- level;
- component;
- operation name;
- message;
- pid/uid/euid;
- optional session/run/job ids;
- optional outcome/error/elapsed_ms;
- structured fields.

Это особенно полезно для разборов таймаутов и “где именно подвисло”.

## Version/build identity

`agentd version` теперь показывает:

- `version`
- `commit`
- `tree`
- `build_id`
- путь к бинарю
- release/update информацию

`build_id` важен для dirty builds: иначе два разных локальных бинаря могли выглядеть одинаково по `commit + tree=dirty`.

## Daemon compatibility checks

HTTP client при подключении к daemon сравнивает:

- version
- commit
- tree state
- build id
- data dir

Это защищает от ситуации “CLI думает, что разговаривает со своим daemon, а на самом деле там другой локальный dirty build”.

## Полезные operator команды

```bash
agentd version
agentd status
agentd logs 200
agentd update
```

Через TUI также доступны:

- `\версия`
- `\логи [N]`
- `\отладка`

## Что смотреть при проблемах

### Если TUI долго открывает session

Смотрите:

- `session_transcript.start`
- `session_transcript.opened_store`
- `session_transcript.loaded_transcripts`
- `pending_approvals.start`
- `pending_approvals.loaded_runs`

Если между `start` и `opened_store` большая пауза — проблема в request-path store open. Если пауза между `pending_approvals.start` и `loaded_runs` — проблема обычно в SQLite contention или в тяжёлом session-scoped scan.

### Если daemon не поднимается или ведёт себя странно

Смотрите:

- `connect_or_autospawn.*`
- `request.start/request.finish/request.error`
- `serve.start/serve.finish`

### Если межагентная цепочка “молчит”

Сначала проверьте:

- создалась ли child session;
- есть ли у неё active jobs/runs;
- не закончился ли hop budget;
- не нужен ли `session_wait`, а не “догадка модели”.

## Кодовые точки

- Store: [`crates/agent-persistence/src/store.rs`](../../crates/agent-persistence/src/store.rs)
- Recovery: [`crates/agent-persistence/src/recovery.rs`](../../crates/agent-persistence/src/recovery.rs)
- Audit log: [`crates/agent-persistence/src/audit.rs`](../../crates/agent-persistence/src/audit.rs)
- Diagnostics builder: [`cmd/agentd/src/diagnostics.rs`](../../cmd/agentd/src/diagnostics.rs)
- Daemon client compatibility logic: [`cmd/agentd/src/http/client.rs`](../../cmd/agentd/src/http/client.rs)
