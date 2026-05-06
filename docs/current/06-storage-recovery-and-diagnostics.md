# Хранилище, recovery и диагностика

## Store layout

Главный storage слой — [`PersistenceStore`](../../crates/agent-persistence/src/store.rs).

По умолчанию layout строится от `data_dir` и содержит:

- PostgreSQL — metadata/control plane: sessions, runs, jobs, schedules, ledgers, search indexes, KV, Telegram bindings
- `artifacts/` — бинарные payload’ы
- `archives/` — архивы сессий
- `agents/` — agent home directories, которые bootstrap создаёт рядом с store
- `runs/` — run-related payload storage
- `transcripts/` — transcript payload storage, новые записи группируются по `session_id`
- `audit/runtime.jsonl` — structured diagnostic log

То есть состояние — это не “только PostgreSQL”. PostgreSQL хранит control-plane metadata и индексы, а большие тела лежат рядом на файловой системе как payload-файлы. Начиная с релиза 1.2.0 старый embedded local DB не является поддерживаемым runtime store: `agentd` не открывает старые DB-файлы, не мигрирует их и не держит rollback-путь.

## Production layout после `scripts/deploy-teamd.sh`

Скрипт установки по умолчанию разделяет config и runtime state:

```text
/etc/teamd/
├── config.toml
└── teamd.env

/var/lib/teamd/state/
├── agents/
├── agent-templates/
├── archives/
├── artifacts/
├── audit/
├── runs/
├── transcripts/
└── USER.md

/var/lib/teamd/knowledge/silverbullet/teamd/
├── a/teamd-agents.md
├── journals/YYYY-MM-DD.md
└── p/teamd-session-<session_id>.md
```

`/etc/teamd/config.toml` — основной TOML-конфиг без секретов. Там задаются `data_dir`, daemon bind address/port, Telegram enable flag, provider kind/base/model и permission mode.

`/etc/teamd/teamd.env` — environment file для systemd unit’ов и операторских CLI-команд. Там лежат секреты и env overrides: `TEAMD_CONFIG`, `TEAMD_DATA_DIR`, `TEAMD_DATABASE_URL`, `TEAMD_TELEGRAM_BOT_TOKEN`, `TEAMD_PROVIDER_API_KEY`. Deploy script создаёт файл как `root:teamd 0640`: `systemd` читает его через `EnvironmentFile`, а операторский helper `teamdctl` использует тот же env, PostgreSQL URL и state root.

`/var/lib/teamd/state` — runtime `data_dir`. Если daemon, Telegram worker и CLI смотрят в разные `data_dir`, вы получите разные sessions, pairings и transcripts. Поэтому для production-like запуска все systemd services и ручные команды должны использовать один и тот же `TEAMD_DATA_DIR=/var/lib/teamd/state`.

`/var/lib/teamd/knowledge/silverbullet/teamd` — external knowledge space, если включён SilverBullet add-on. Это не `data_dir`: runtime state остаётся в PostgreSQL + payload layout, а SilverBullet получает daily notes и best-effort human-readable session mirrors.

## Что лежит в `data_dir`

| Path | Что это | Можно ли редактировать руками |
| --- | --- | --- |
| `agent-templates/<template_id>/SYSTEM.md` | Runtime-editable template system prompt. Используется для bootstrap/update generated agent homes. Если файла нет, daemon создаёт его из bundled repo template. | Да. Правка влияет на будущий bootstrap/sync, но не обязана молча затирать уже вручную изменённый `agent_home`. |
| `agent-templates/<template_id>/AGENTS.md` | Runtime-editable template tool/profile guidance. | Да. Используйте для изменения template без пересборки `agentd`. |
| `agent-templates/<template_id>/skills/` | Runtime-editable template skills. | Да. Это правильное место для правки shipped skills без rebuild. |
| `agent-templates/system/memory-curator/SYSTEM.md` | Runtime-editable prompt для post-turn memory curator. Это не agent profile и не skill, а системный короткий provider-вызов для извлечения durable memory candidates. | Да. Правка меняет поведение curator без пересборки `agentd`. |
| `USER.md` | Operator context: язык, timezone, правила интерпретации времени и устойчивые операторские настройки. Runtime создаёт файл из template при первом чтении. | Да. Влияет на будущие turns всех agent profiles на этом runtime node. |
| `agents/<agent_id>/SYSTEM.md` | System prompt конкретного agent profile. Runtime читает его при сборке prompt. | Да, осознанно. Влияет на будущие turns этого agent profile. |
| `agents/<agent_id>/AGENTS.md` | Инструкции и tool-usage guidance конкретного agent profile. | Да, осознанно. Влияет на будущие turns. |
| `agents/<agent_id>/skills/` | Локальные skills для agent profile. | Да, если понимаете формат skills. |
| `archives/` | Архивированные sessions и их payload’ы. Для session archive используется `archives/sessions/<session_id>/`. | Обычно нет. Лучше читать через runtime commands. |
| `artifacts/` | Binary payload files для artifacts/context offload. Файл обычно называется `<artifact_id>.bin`. | Нет. Integrity проверяется через PostgreSQL metadata. |
| `audit/runtime.jsonl` | Append-only diagnostic events: bootstrap, HTTP requests, daemon lifecycle, Telegram worker, provider loop и ошибки. | Читать можно. Редактировать не нужно. |
| `runs/` | Run-related payload directory, создаётся layout’ом. Большая часть run state сейчас хранится в PostgreSQL. | Нет. |
| `transcripts/<session_id>/` | Text payload files для новых transcript entries. PostgreSQL хранит index и hash, файл хранит body сообщения. Старые flat-файлы в `transcripts/` остаются читаемыми для обратной совместимости. | Нет. |

Пример transcript payload:

```text
transcripts/session-1777036286947/transcript-run-chat-session-1777036286947-1777036286-01-user.txt
transcripts/session-1777036286947/transcript-run-chat-session-1777036286947-1777036286-02-assistant.txt
```

Storage key теперь обычно содержит `session_id/filename.txt`. Смысл записи хранится не только в имени, а в PostgreSQL: `session_id`, `run_id`, `kind`, `created_at`, `byte_len`, `sha256`.

Важно: `agents/<agent_id>/` сейчас является `agent_home`, а не project workspace. Там лежат prompt-файлы и skills профиля агента. Рабочая директория выполнения tool’ов пока приходит из runtime/session/schedule context и требует отдельной модернизации. План описан в [11-workspace-modernization-plan.md](11-workspace-modernization-plan.md).

## Что лежит в SilverBullet Space

SilverBullet Space конфигурируется через `[knowledge].silverbullet_space_dir` или `TEAMD_SILVERBULLET_SPACE_DIR`. Production default:

```text
/var/lib/teamd/knowledge/silverbullet/teamd
```

Ключевые paths:

| Path | Что это | Источник истины |
| --- | --- | --- |
| `journals/YYYY-MM-DD.md` | Daily journal оператора/агента. Today/yesterday excerpts могут попадать в `SessionHead`. | SilverBullet note. |
| `a/teamd-agents.md` | Area index для generated session mirror pages. | SilverBullet mirror/index, не runtime state. |
| `p/teamd-session-<session_id>.md` | Best-effort mirror сессии: plan snapshot, context summary, recent tool activity, artifacts. | `agentd` остаётся source of truth; page только view. |

Mirror pages можно читать и дополнять операторскими заметками, но ручная правка mirror не меняет runtime plan/transcripts/tool calls. Если нужно изменить runtime state, используйте соответствующие teamD tools/commands.

## Жизненный цикл session

Session — durable business entity. Она не удаляется и не архивируется автоматически только потому, что оператор давно не писал в чат. Пока нет явной команды удаления/archive или retention policy, session остаётся активной в PostgreSQL, а transcript/tool ledger/artifacts остаются доступными через CLI/TUI/Telegram debug surfaces.

Что может происходить без нового сообщения оператора:

- Telegram worker доставляет queued session notifications и file delivery requests.
- Scheduler будит session через `session_inbox_events`/jobs, если агент явно создал schedule или `continue_later`.
- Background job worker продолжает уже queued work.
- Recovery при рестарте честно помечает оборванные runs как interrupted, если их нельзя восстановить.

Что runtime не делает сам:

- не “пинает” агента только из-за незавершённого plan;
- не продолжает план без queued/scheduled work;
- не удаляет artifacts/transcripts из-за неактивности без отдельной retention policy;
- не считает SilverBullet mirror источником истины для plan/tool calls.

Операторский `/status` в Telegram показывает lifecycle summary, чтобы было понятно, что session ждёт явного действия, schedule/job или нового сообщения.

## Session artifacts и operator-readable mirror

Канонический artifact store остаётся в PostgreSQL + `artifacts/`. Telegram documents, screenshots, generated files и offload payloads получают `artifact_id`; читать и доставлять их нужно через `agentd session ...`, `/files`, `/file <artifact_id>` или соответствующие tools.

Если включён SilverBullet mirror, runtime best-effort пишет человекочитаемую page:

```text
/var/lib/teamd/knowledge/silverbullet/teamd/p/teamd-session-<session_id>.md
```

Mirror должен помогать оператору видеть plan snapshot, context summary, tool activity и artifacts в браузере. Он не заменяет canonical ledger: ручная правка mirror не меняет runtime plan, transcript или result ledger.

## Что хранит PostgreSQL

PostgreSQL — это не payload store, а metadata/control plane. Основные таблицы:

| Table | Что хранит |
| --- | --- |
| `sessions` | Сессии, title, settings, выбранный `agent_profile_id`, parent/delegation metadata. |
| `missions` | Долгоживущие цели, status, schedule и acceptance criteria. |
| `runs` | Execution runs: status, provider usage, recent steps, pending approvals, provider loop state, delegates. |
| `jobs` | Очередь фоновой работы: chat turns, scheduled work, callbacks, leases, attempts, cancellation. |
| `transcripts` | Индекс transcript payload files в `transcripts/`: role/kind, session/run links, storage key, hash. |
| `tool_calls` | Ledger вызовов tools: provider call id, tool name, arguments JSON, summary, status, error, timestamps, result summary, result preview и ссылку на artifact полного результата. |
| `artifacts` | Индекс artifact payload files в `artifacts/`: kind, path, metadata, size, hash. |
| `file_delivery_requests` | Durable очередь доставки файлов текущему оператору: session/run links, artifact id, target, filename, caption, status, timestamps и error. Telegram worker вычитывает queued rows и отправляет их как documents. |
| `agent_profiles` | Agent profiles, template kind, allowed tools, путь к `agent_home`. |
| `agent_schedules` | Deferred/recurring schedules для agent profiles. |
| `agent_chain_continuations` | Grants для inter-agent chain continuation. |
| `plans` | Structured plan items по session. |
| `context_summaries` | Compact summaries старого transcript tail по session. |
| `context_offloads` | Ссылки на offloaded context chunks, payload лежит в `artifacts/`. |
| `kv_entries` | Scoped deterministic KV: `scope`, `namespace_id`, `key`, JSON value, metadata, revision, timestamps и optional TTL. |
| `session_inbox_events` | Deferred wakeups, inbox events и продолжения работы. |
| `session_retention` | Retention/archive metadata по sessions. |
| `session_search_docs` | PostgreSQL full-text search index по session history. |
| `knowledge_sources`, `knowledge_search_docs` | Indexed knowledge/docs для поиска через PostgreSQL full-text search. |
| `mcp_connectors` | Configured MCP connectors и их persisted config. |
| `telegram_user_pairings` | Pending/activated Telegram pairing keys и Telegram user metadata. |
| `telegram_chat_bindings` | Привязка Telegram chat к выбранной session и delivery cursor. |
| `telegram_update_cursors` | Last processed Telegram update id для long polling consumer. |
| `daemon_state` | Небольшой key/value state daemon-level настроек. |

## Два режима открытия store

Это один из самых важных недавних runtime-fix’ов.

### `PersistenceStore::open(...)`

Используется для bootstrap path:

- готовит layout;
- подключается к PostgreSQL;
- настраивает соединение;
- bootstrap’ит и валидирует schema;
- делает reconcile orphan payloads.

### `PersistenceStore::open_runtime(...)`

Используется в request path:

- открывает уже готовую БД;
- не делает тяжёлую bootstrap/reconcile work;
- нужен для быстрых HTTP/TUI reads.

Это разделение устраняет зависания вроде “открытие transcript-tail внезапно 10 секунд думает”.

## Атомарность multi-step store paths

В storage слое есть несколько путей, где логически одна операция требует нескольких SQL statement’ов: перестройка search-индекса, очистка индекса при delete, замена Telegram pairing key.

Для подтверждённых hot-path операций runtime использует PostgreSQL transactions и advisory locks, чтобы не открывать окна гонки между отдельными statement’ами:

- `replace_session_search_docs`
- `replace_knowledge_search_docs`
- `put_telegram_user_pairing`
- `delete_knowledge_source`
- `delete_session` (DB-часть; payload cleanup остаётся после commit)

Это убирает два класса проблем:

- transient unique constraint conflicts при конкурентной замене одних и тех же logical rows;
- частично видимые состояния вида “metadata/search docs ещё есть, а индекс уже удалён” между независимыми autocommit statement’ами.

Если вы ловите новый constraint error или странный search mismatch, первым делом смотрите, не появился ли ещё один multi-step mutation path без общей транзакции/advisory lock.

## PostgreSQL runtime политика

Сейчас для PostgreSQL явно настроены:

- connection timeout через `[database].connect_timeout_seconds`;
- application name через `[database].application_name`;
- retry на transient PostgreSQL errors в request-path/runtime-path hot spots: store open, tool-call ledger/result persistence и Telegram polling/binding updates.

Retry policy настраивается через `runtime_timing.store_retry_delay_ms` и `runtime_limits.store_retry_attempts`. Runtime считает transient только SQLSTATE-классы, которые можно безопасно повторить: serialization failure, deadlock, lock-not-available и краткие connection failures. Это не заменяет короткие транзакции и правильные индексы, а закрывает короткие окна contention, которые раньше протекали в пользовательские ошибки.

## PostgreSQL-only политика 1.2.0

Релиз 1.2.0 считает PostgreSQL единственным поддерживаемым control-plane store. Старые локальные DB-файлы из до-PostgreSQL установок можно хранить как внешний backup, но runtime их не читает и CLI больше не содержит встроенную команду миграции.

Практический порядок для production:

- `TEAMD_DATABASE_URL` обязателен для daemon, Telegram worker, `teamdctl` и любых ручных operator commands.
- `data_dir` остаётся владельцем payload directories (`artifacts/`, `transcripts/`, `archives/`, `runs/`) и agent homes.
- Перед переносом старого узла делайте полный backup PostgreSQL + `data_dir`; rollback через старый embedded store больше не является поддерживаемым сценарием.

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

Диагностика процесса пишется structured JSON events в:

- `data_dir/audit/runtime.jsonl`

Через `DiagnosticEventBuilder` события получают:

- timestamp;
- level;
- component;
- operation name;
- message;
- pid/uid/euid;
- optional session/run/job ids;
- optional trace/span ids;
- optional surface/entrypoint;
- optional outcome/error/elapsed_ms;
- structured fields.

Это особенно полезно для разборов таймаутов и “где именно подвисло”.

`audit/runtime.jsonl` — это лог runtime/daemon/surface-слоёв, а не transcript агента. Там есть события bootstrap, HTTP client/server, daemon lifecycle, Telegram worker, provider loop, ошибки и timing. Сообщения пользователя/ассистента лежат в `transcripts/` и индексируются в таблице `transcripts`.

Команда:

```bash
agentd logs 200
```

читает последние строки именно из `audit/runtime.jsonl`. То есть `agentd logs` — это diagnostic logs `agentd`, а не “логи одного агента”.

Быстрый агрегированный срез по diagnostics и tool ledger:

```bash
agentd analytics 200
```

Для systemd-установки:

```bash
teamdctl analytics 200
```

`analytics` показывает количество audit events в выбранном tail, группировки по `level`, `surface`, `component`, `op`, Telegram delivery attempts/failures/average latency и сводку tool calls по PostgreSQL ledger. Это локальная аналитика, а не внешний OpenTelemetry exporter.

Локальный trace-view:

```bash
agentd trace run <run_id>
agentd trace show <trace_id>
agentd trace export <trace_id>
agentd trace push <trace_id>
```

Для systemd-установки:

```bash
teamdctl trace run <run_id>
teamdctl trace show <trace_id>
teamdctl trace export <trace_id>
teamdctl trace push <trace_id>
```

Trace хранится в PostgreSQL таблице `trace_links`. Она связывает доменные сущности (`run`, `provider_round`, `transcript`, `tool_call`, `artifact`) с `trace_id`, `span_id`, `parent_span_id`, `surface`, `entrypoint` и компактными attributes. `trace run` удобен после `session tools`, потому у каждого tool call виден `run_id`. `trace export` печатает OTLP-compatible JSON payload без сетевой отправки. `trace push` отправляет этот payload в configured OTLP/HTTP endpoint.

Auto-export включается конфигом:

```toml
[observability]
otlp_export_enabled = true
otlp_endpoint = "http://127.0.0.1:4318/v1/traces"
otlp_timeout_ms = 2000
```

Для production-like установки с локальным web UI используйте:

```bash
./scripts/deploy-teamd-containers.sh --with-jaeger
```

Сбой OTLP/Jaeger экспорта не должен ломать пользовательский turn. Runtime пишет диагностическое событие `component=otel`, `op=export`, `outcome=error` в `audit/runtime.jsonl`, а локальные `trace_links`, transcripts, tool calls и artifacts остаются источником истины.

В установке через `deploy-teamd.sh` создаются два operator entrypoint:

- `/usr/local/bin/agentd` — symlink на `/opt/teamd/bin/agentd`, удобен для ручного локального запуска от текущего пользователя;
- `/usr/local/bin/teamdctl` — helper для production-state: читает `/etc/teamd/teamd.env`, переключается на пользователя `teamd` и запускает `/opt/teamd/bin/agentd`.

Если binary ставился вручную, зарегистрировать `agentd` в `PATH` можно так:

```bash
sudo mkdir -p /usr/local/bin
sudo ln -sf /opt/teamd/bin/agentd /usr/local/bin/agentd
hash -r
agentd version
```

`journalctl` показывает stdout/stderr systemd unit’ов, а `agentd logs` показывает structured audit file из `data_dir`. Обычно нужны оба источника:

```bash
teamdctl daemon logs
teamdctl telegram logs
teamdctl logs 200
```

## Чтение сессий и tool calls

Для оператора есть CLI-команды поверх того же store:

```bash
agentd session list
agentd sessions
agentd session list --raw
agentd session transcript <session_id>
agentd session tools <session_id> --limit 50 --offset 0
agentd session tools <session_id> --results --limit 50 --offset 0
agentd session tool-result <tool_call_id>
```

`session list` показывает список sessions. По умолчанию это человекочитаемый отчёт: title, `session_id`, agent profile, timestamps, message count, context/usage, pending approval flag, auto-approve flag, background job counts, schedule summary и последний preview. Alias `sessions` делает то же самое.

Если нужен старый однострочный формат для `grep`, diff или внешних скриптов:

```bash
agentd session list --raw
agentd sessions --raw
```

`session transcript` рендерит transcript view сессии. Это удобнее, чем вручную смотреть payload-файлы в `transcripts/<session_id>/`, потому команда берёт порядок, роли и связи из PostgreSQL.

`session tools` рендерит ledger вызовов tools по сессии. Он нужен для аудита и улучшения инструкций агентам: видно, какой tool был запрошен, с какими аргументами, в каком run, чем закончился вызов и была ли ошибка.

`session tools` постраничный: по умолчанию показывает до 50 записей, а в заголовке печатает `total`, `showing`, `limit`, `offset` и `next_offset`. Обычный вывод рассчитан на человека: вызовы сгруппированы по `run`, каждый вызов имеет номер, ISO-время вызова, `summary`, pretty-printed `args`, `status` и `error`.

По умолчанию список не печатает tool output, чтобы оставаться читаемым. Для глубокого debug используйте `--results`: рядом с каждым вызовом появятся `result_summary`, `result_byte_len`, `result_truncated`, `result_artifact_id` и bounded preview. Для полного output одного вызова используйте `session tool-result <tool_call_id>`. Если результат крупный, команда прочитает payload из `artifacts`; если маленький, покажет сохранённый preview целиком.

В TUI для той же задачи есть интерактивный `Debug` browser:

- на экране списка sessions выберите session и нажмите `Д`;
- внутри chat нажмите `Ctrl+D`;
- или выполните команду `\дебаг`.

Debug browser показывает единый timeline из сообщений, tool calls и artifacts. Слева список записей, справа детали выбранной записи. `↑/↓` меняют запись, `Enter` открывает текущие детали на весь экран, `/` ищет по detail pane, `n/N` переходят по совпадениям. Это не отдельный runtime path: локальный TUI читает `App::session_debug_view`, daemon-backed TUI читает тот же view через `GET /v1/sessions/<session_id>/debug`.

Команда `\отладка` отличается от `\дебаг`: она сохраняет текстовый debug bundle в файл. Daemon-side bundle пишется не в workspace, а в runtime state: `DATA_DIR/audit/debug-bundles/<session_id>-<timestamp>.txt`. Для systemd-установки это обычно `/var/lib/teamd/state/audit/debug-bundles/...`. Такое размещение важно для сервисного режима: daemon всегда пишет в свою state-директорию и не зависит от прав текущего workspace.

Следующую страницу можно запросить так:

```bash
agentd session tools <session_id> --limit 50 --offset <next_offset>
```

Полный результат конкретного tool call:

```bash
agentd session tool-result <tool_call_id>
agentd session tool-result <tool_call_id> --raw
```

Для машинного аудита, `grep` и старых скриптов есть однострочный формат:

```bash
agentd session tools <session_id> --raw --limit 50 --offset 0
```

Для production-like systemd:

```bash
teamdctl session list
teamdctl session list --raw
teamdctl session transcript <session_id>
teamdctl session tools <session_id> --limit 50 --offset 0
teamdctl session tools <session_id> --results --limit 50 --offset 0
teamdctl session tool-result <tool_call_id>
teamdctl session tools <session_id> --raw --limit 50 --offset 0
```

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
- `\дебаг` — интерактивный просмотр session debug-view
- `\отладка` — сохранить debug bundle в `DATA_DIR/audit/debug-bundles`

## Что смотреть при проблемах

### Если TUI долго открывает session

Смотрите:

- `session_transcript.start`
- `session_transcript.opened_store`
- `session_transcript.loaded_transcripts`
- `pending_approvals.start`
- `pending_approvals.loaded_runs`

Если между `start` и `opened_store` большая пауза — проблема в request-path store open или PostgreSQL connectivity. Если пауза между `pending_approvals.start` и `loaded_runs` — проблема обычно в PostgreSQL contention/query latency или в тяжёлом session-scoped scan.

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
