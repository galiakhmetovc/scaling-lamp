# Auto Context Compaction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Добавить автоматическую compaction перед provider turn при достижении заданной доли context window.

**Architecture:** Auto-compaction встраивается в канонический `execute_provider_turn_loop` только для новых turn'ов. Trigger считает размер реально собранного prompt, а сама compaction остаётся той же самой summary-механикой, что и ручная.

**Tech Stack:** Rust, `agent-runtime`, `agent-persistence`, bootstrap integration tests

---

### Task 1: Зафиксировать конфигурацию и trigger contract

**Files:**
- Modify: `crates/agent-persistence/src/config.rs`
- Test: `crates/agent-persistence/src/config/tests.rs`
- Modify: `config.example.toml`

- [ ] **Step 1: Добавить failing config tests**
- [ ] **Step 2: Добавить новые context config fields и env overrides**
- [ ] **Step 3: Провалидировать ratio и context window override**
- [ ] **Step 4: Обновить `config.example.toml`**

### Task 2: Добавить failing execution tests

**Files:**
- Modify: `cmd/agentd/tests/bootstrap_app/context.rs`

- [ ] **Step 1: Написать тест на auto-compaction перед chat turn**
- [ ] **Step 2: Написать тест на отсутствие auto-compaction ниже порога**
- [ ] **Step 3: Написать тест на reuse того же trigger в background/wakeup path**
- [ ] **Step 4: Запустить эти тесты и убедиться, что они падают**

### Task 3: Реализовать канонический auto-compaction path

**Files:**
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`

- [ ] **Step 1: Протянуть новые context settings в `ExecutionServiceConfig`**
- [ ] **Step 2: Добавить resolver context window tokens**
- [ ] **Step 3: Добавить estimate prompt tokens helper**
- [ ] **Step 4: Добавить `maybe_auto_compact_session_before_turn()`**
- [ ] **Step 5: Встроить trigger в `execute_provider_turn_loop` только для новых turn'ов**
- [ ] **Step 6: Убрать дублирование compaction logic между manual и auto path**

### Task 4: Обновить operator-facing docs

**Files:**
- Modify: `docs/current/07-config.md`
- Modify: `docs/current/02-prompt-and-turn-flow.md`
- Modify: `cmd/agentd/src/help.rs`

- [ ] **Step 1: Обновить описание `[context]`**
- [ ] **Step 2: Убрать утверждение, что compaction только manual-only**
- [ ] **Step 3: Уточнить, что manual compaction остаётся доступной**

### Task 5: Полная проверка

**Files:**
- None

- [ ] **Step 1: `cargo fmt --all`**
- [ ] **Step 2: `cargo clippy --workspace --all-targets --all-features -- -D warnings`**
- [ ] **Step 3: `cargo test --workspace --all-features`**
- [ ] **Step 4: `cargo build -p agentd`**
- [ ] **Step 5: `cargo build --release -p agentd`**
