# PostgreSQL Runtime Store Design

## Goal

Перевести canonical TeamD runtime store с SQLite на PostgreSQL так, чтобы daemon, Telegram worker, CLI, TUI, schedules, tool ledger, KV, traces, sessions, transcripts metadata и artifacts metadata использовали один PostgreSQL-backed control plane.

Payload-файлы остаются файловыми: transcripts, artifacts, archives и audit лежат в `data_dir`, а PostgreSQL хранит durable metadata, индексы и runtime state.

## Why

SQLite WAL/lock contention на production уже приводит к:

- `database is locked` в chat turns и CLI;
- разрастанию `state.sqlite-wal`;
- CPU/read storm внутри daemon;
- невозможности безопасно обслуживать Telegram, daemon background workers и operator diagnostics одновременно.

PostgreSQL нужен как один durable multi-client runtime store с нормальной конкурентностью, row-level locking, отдельной observability surface и predictable operational model.

## Non-Goals

- Не менять canonical chat loop, prompt path или tool loop.
- Не переносить binary payloads в PostgreSQL.
- Не заменять Mem0, SilverBullet или KV их внешними аналогами.
- Не вводить второй runtime path для Telegram/TUI/CLI.

## Target Architecture

### Config

Добавляется секция:

```toml
[database]
url = "postgresql://teamd:teamd@127.0.0.1:5432/teamd"
connect_timeout_seconds = 5
application_name = "teamd"
```

Env overrides:

- `TEAMD_DATABASE_URL`
- `TEAMD_DATABASE_CONNECT_TIMEOUT_SECONDS`
- `TEAMD_DATABASE_APPLICATION_NAME`

`data_dir` остаётся обязательным: он определяет payload layout, audit path, agent homes, workspaces and local templates.

### Store

`PersistenceStore` становится PostgreSQL-backed runtime store:

- открывает PostgreSQL connection через config;
- bootstrap создаёт PostgreSQL schema;
- runtime open не делает тяжёлую миграцию, но проверяет доступность подключения;
- repository traits остаются публичным контрактом для app/runtime layer;
- payload helpers остаются файловыми и backend-agnostic.

### Schema

SQLite tables переводятся в PostgreSQL DDL:

- `TEXT`, `BIGINT`, `BOOLEAN`;
- `ON CONFLICT ... DO UPDATE`;
- foreign keys with `ON DELETE`;
- indexes under explicit names;
- FTS через PostgreSQL `tsvector`/`tsquery` or fallback `ILIKE` for current `knowledge_search`/`session_search` behavior.

### Transactions

Multi-step mutations become PostgreSQL transactions:

- `put_kv_entry` and `delete_kv_entry` use `SELECT ... FOR UPDATE` for CAS revision checks;
- Telegram pairing delete+insert runs in one transaction;
- session delete/archive cleanup stays atomic at metadata level and best-effort for payload cleanup;
- search doc replacement deletes old docs and inserts new docs in one transaction.

### Legacy Migration

Production migration needs explicit operator command:

```bash
agentd migrate sqlite-to-postgres \
  --sqlite /var/lib/teamd/state/state.sqlite \
  --database-url "$TEAMD_DATABASE_URL"
```

It copies metadata tables into PostgreSQL and leaves payload files in place. It does not delete SQLite files. Deploy can call this once after backup.

### Deploy

The core deploy script installs PostgreSQL runtime prerequisites and writes:

- `/etc/teamd/config.toml` with `[database]`;
- `/etc/teamd/teamd.env` with `TEAMD_DATABASE_URL`;
- systemd units unchanged except shared env file.

Container add-ons may optionally manage PostgreSQL, but TeamD daemon itself stays host systemd process.

## Validation

Required gates:

- PostgreSQL integration store tests for sessions/runs/transcripts/tools/KV/Telegram/schedules/search.
- Migration smoke test from a SQLite fixture to PostgreSQL.
- `cargo fmt --all`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo build --release -p agentd`

## Rollout

1. Stop TeamD services.
2. Backup `/var/lib/teamd/state`, `/etc/teamd`, and PostgreSQL if already exists.
3. Start PostgreSQL.
4. Run schema bootstrap.
5. Run SQLite-to-PostgreSQL migration.
6. Start daemon.
7. Validate `/v1/status`, `teamdctl session list`, Telegram `/status`, one chat turn, tool ledger and schedule list.
8. Keep old `state.sqlite*` files for rollback until the operator explicitly removes them.
