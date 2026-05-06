# PostgreSQL Runtime Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace TeamD production runtime metadata/control-plane storage with PostgreSQL while preserving the canonical runtime path and file-backed payload layout.

**Architecture:** `PersistenceStore` becomes PostgreSQL-backed and keeps the existing repository trait contract. `data_dir` continues to own payload files, audit, agents and workspaces; PostgreSQL owns sessions, runs, jobs, schedules, KV, tool ledger, traces, Telegram bindings and search metadata. A one-shot migration command copies existing `state.sqlite` metadata into PostgreSQL without deleting rollback files.

**Tech Stack:** Rust 1.95, `postgres` crate, PostgreSQL 16, existing `agent-persistence` repositories, systemd deploy scripts, cargo tests.

---

### Task 1: Database Config

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/agent-persistence/Cargo.toml`
- Modify: `crates/agent-persistence/src/config.rs`
- Modify: `crates/agent-persistence/src/lib.rs`
- Modify: `crates/agent-persistence/src/config/tests.rs`

- [ ] Add `postgres` workspace dependency.
- [ ] Add `DatabaseConfig { url, connect_timeout_seconds, application_name }`.
- [ ] Add TOML and env parsing for `TEAMD_DATABASE_URL`.
- [ ] Keep `data_dir` separate from database config.
- [ ] Add config tests for TOML + env override precedence.

### Task 2: PostgreSQL Schema

**Files:**
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store.rs`

- [ ] Replace SQLite DDL with PostgreSQL DDL.
- [ ] Replace FTS5 virtual tables with PostgreSQL-compatible search tables/indexes.
- [ ] Replace schema validation based on PRAGMA with `information_schema`/`pg_catalog`.
- [ ] Preserve all foreign keys and indexes.

### Task 3: Store Connection Layer

**Files:**
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/store/payloads.rs`

- [ ] Replace `rusqlite::Connection` runtime connection with PostgreSQL client wrapper.
- [ ] Add `StoreError::Postgres`.
- [ ] Add transaction helper for multi-step writes.
- [ ] Keep payload path/integrity helpers unchanged.

### Task 4: Repository Port

**Files:**
- Modify: `crates/agent-persistence/src/store/session_mission.rs`
- Modify: `crates/agent-persistence/src/store/execution_repos.rs`
- Modify: `crates/agent-persistence/src/store/tool_call_repos.rs`
- Modify: `crates/agent-persistence/src/store/trace_repos.rs`
- Modify: `crates/agent-persistence/src/store/inbox_repos.rs`
- Modify: `crates/agent-persistence/src/store/agent_repos.rs`
- Modify: `crates/agent-persistence/src/store/telegram_repos.rs`
- Modify: `crates/agent-persistence/src/store/kv_repos.rs`
- Modify: `crates/agent-persistence/src/store/context_repos.rs`
- Modify: `crates/agent-persistence/src/store/memory_repos.rs`
- Modify: `crates/agent-persistence/src/store/mcp_repos.rs`
- Modify: `crates/agent-persistence/src/store/file_delivery_repos.rs`

- [ ] Convert placeholders from `?N` to `$N`.
- [ ] Convert row mappers to PostgreSQL row types.
- [ ] Convert integer booleans to `BOOLEAN`.
- [ ] Convert CAS/replace paths to PostgreSQL transactions.
- [ ] Keep repository traits unchanged.

### Task 5: Migration Command

**Files:**
- Modify: `cmd/agentd/src/cli.rs`
- Create: `cmd/agentd/src/migrate.rs`
- Modify: `cmd/agentd/Cargo.toml`

- [ ] Add `agentd migrate sqlite-to-postgres`.
- [ ] Read legacy SQLite read-only.
- [ ] Bootstrap PostgreSQL schema.
- [ ] Copy all metadata tables in dependency order.
- [ ] Keep payload files in current `data_dir`.
- [ ] Emit counts and stop on mismatch.

### Task 6: Tests

**Files:**
- Modify: `crates/agent-persistence/src/store/tests.rs`
- Modify: `crates/agent-persistence/src/store/tests/*.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/*.rs`
- Create: `cmd/agentd/tests/postgres_migration.rs`

- [ ] Add PostgreSQL test database helper.
- [ ] Port store tests to PostgreSQL.
- [ ] Add migration smoke test.
- [ ] Update bootstrap tests that directly inspected SQLite.
- [ ] Keep tests isolated by database/schema name.

### Task 7: Deploy and Docs

**Files:**
- Modify: `scripts/deploy-teamd.sh`
- Modify: `scripts/deploy-teamd-containers.sh`
- Modify: `scripts/test-deploy-teamd.sh`
- Modify: `docs/current/06-storage-recovery-and-diagnostics.md`
- Modify: `docs/current/07-config.md`
- Modify: `docs/current/17-runtime-mental-model.md`

- [ ] Install/check PostgreSQL client/server prerequisites.
- [ ] Write `[database]` config.
- [ ] Document backup/migration/rollback.
- [ ] Remove `state.sqlite` as production source of truth from docs.

### Task 8: Quality Gate

- [ ] Run `cargo fmt --all`.
- [ ] Run PostgreSQL integration tests.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace --all-features`.
- [ ] Run `cargo build --release -p agentd`.
- [ ] Update beads issue `teamD-bp9` with validation notes.
