# Diagnostics Audit Design

## Goal

Добавить один канонический слой подробной диагностики, чтобы оператор мог быстро понять:

- с каким `data_dir` и какими правами стартовал runtime;
- какой локальный daemon был переиспользован, перезапущен или отклонён;
- на каком HTTP/storage/session path произошёл timeout, `PermissionDenied` или другой сбой;
- какой минимальный хвост логов надо приложить в баг-репорт или debug bundle.

## Non-Goals

- Не вводить второй runtime path, второй prompt path или отдельный special daemon/UI loop.
- Не тащить полноценный `tracing` stack как отдельный большой refactor.
- Не засыпать код ad-hoc `println!` сообщениями.
- Не писать в stdout/stderr подробные логи по умолчанию.

## Existing Baseline

У runtime уже есть канонический audit path:

- `data_dir/audit/runtime.jsonl`

Но он пока не является общим детализированным diagnostic spine. Отдельные debug surfaces есть, но они фрагментированы:

- `StatusResponse`
- debug bundle
- TUI `\отладка`
- системные timeline/status сообщения

## Design

### 1. Canonical Diagnostic Log

Добавляется единый structured JSONL log, пишущийся только в:

- `data_dir/audit/runtime.jsonl`

Каждая запись должна быть самостоятельным event’ом с плоской структурой, пригодной для grep/jq и для включения в debug bundle.

Минимальные поля:

- `ts`
- `level`
- `component`
- `op`
- `message`
- `pid`
- `uid`
- `euid`
- `data_dir`
- `session_id`
- `run_id`
- `job_id`
- `daemon_base_url`
- `elapsed_ms`
- `outcome`
- `error`
- `fields` — дополнительный object для path/method/status/etc.

### 2. Logging Philosophy

Диагностика должна отвечать на три практических вопроса:

1. Что runtime собирался сделать?
2. Что именно он решил на ветвлении?
3. Чем это закончилось и сколько заняло?

Поэтому логирование строится как bounded lifecycle events:

- `start`
- `decision`
- `finish`
- `error`

а не как бессистемный поток строк.

### 3. First Instrumentation Targets

#### Config / identity

- capture `HOME`, `XDG_STATE_HOME`, `TEAMD_DATA_DIR`
- computed `data_dir`
- `uid/euid`
- current executable path

#### Local daemon reuse / autospawn

- status probe start/finish
- compatibility decision:
  - build mismatch
  - `data_dir` mismatch
  - remote target mismatch
- shutdown/restart decisions
- spawn start/finish
- final connected daemon identity

#### HTTP client / daemon routes

- `GET /v1/status`
- `GET /v1/sessions`
- `GET /v1/sessions/{id}`
- `POST /v1/sessions`
- `DELETE /v1/sessions/{id}`
- `POST /v1/sessions/{id}/clear`

Логировать:

- method
- path
- timeout
- HTTP status
- elapsed
- key ids

#### Storage hot paths

- store open
- session list rollups
- session delete payload scan
- transcript/artifact payload enumeration
- archive/missing payload cases
- permission denied / corrupt payload / sqlite errors

#### TUI startup and operator actions

- TUI connect/autospawn path
- session browser refresh
- open/create/delete/clear flows
- explicit debug bundle save

### 4. Operator-Facing Access

Нужен тонкий operator surface поверх того же canonical log:

- CLI/REPL команда просмотра хвоста логов
- TUI команда/экран для быстрого tail
- debug bundle должен включать свежий diagnostic tail

Это должен быть thin surface над `runtime.jsonl`, не отдельный logging store.

### 5. Refactoring Constraints

Instrumentation допускает только целевые refactor’ы:

- вынос повторяющегося request/decision logging в helper’ы;
- вынос тяжёлых compatibility checks в отдельные функции;
- вынос repeated session route instrumentation в обёртки.

Но нельзя:

- дублировать app/runtime flow;
- строить второй audit subsystem;
- размазывать environment-specific heuristics по разным модулям.

## Error Reporting Outcomes

После этого среза у оператора должен получаться полезный bug report без устного восстановления контекста:

- каким бинарём запускали;
- какой `data_dir` был активен;
- какой daemon reuse decision был принят;
- какой route завис;
- какой storage error поднялся под ним;
- хвост диагностического лога за последние события.

## Acceptance Criteria

1. `runtime.jsonl` получает structured entries для config/daemon/session hot paths.
2. Local daemon reuse explicitly distinguishes `version/commit` mismatch and `data_dir` mismatch.
3. Session create/delete/list/open failures оставляют понятный diagnostic trail.
4. Operator can quickly inspect a bounded tail without manually opening the file.
5. Debug bundle includes a recent diagnostic log tail.
