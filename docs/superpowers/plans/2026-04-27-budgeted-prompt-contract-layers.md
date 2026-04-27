# Budgeted Prompt Contract Layers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the prompt contract into budgeted prompt views backed by readable and manageable runtime state.

**Architecture:** The canonical `PromptAssembly` path remains the only prompt path. Each bounded prompt layer is rendered from source-of-truth state, constrained by budgets derived from `usable_context_tokens = context_window_tokens * auto_compaction_trigger_ratio`, and paired with tools or operator surfaces for full read/write access.

**Tech Stack:** Rust, `agent-runtime`, `agent-persistence`, `agentd` bootstrap/execution, markdown docs

---

### Task 1: Update the prompt contract decision record

**Files:**
- Modify: `docs/current/12-prompt-contract-decision.md`

- [x] **Step 1: Replace fixed OffloadRefs limits with budget policy**
- [x] **Step 2: Add `AutonomyState` as a first-class prompt layer**
- [x] **Step 3: Add `RecentToolActivity` as a bounded prompt layer**
- [x] **Step 4: Record that all prompt views need source-of-truth state plus read/write tools where allowed**
- [x] **Step 5: Update D1-D8 summary so it matches implemented auto-compaction**

### Task 2: Add prompt model tests first

**Files:**
- Modify: `crates/agent-runtime/src/prompt.rs`
- Modify: `crates/agent-runtime/src/plan.rs`

- [x] **Step 1: Add a failing test for layer order: skills, SessionHead, AutonomyState, Plan, Summary, OffloadRefs, RecentToolActivity, Tail**
- [x] **Step 2: Add a failing test for compact `PlanPromptView`**
- [x] **Step 3: Add a failing test for `SessionHead` provider/model/context/workspace/agent profile visibility**

### Task 3: Implement the first canonical runtime slice

**Files:**
- Modify: `crates/agent-runtime/src/prompt.rs`
- Modify: `crates/agent-runtime/src/plan.rs`
- Modify: `cmd/agentd/src/prompting.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`

- [x] **Step 1: Add `AutonomyState` and `RecentToolActivity` render types**
- [x] **Step 2: Switch prompt assembly from full plan to compact `PlanPromptView`**
- [x] **Step 3: Extend `SessionHead` with provider/model/think/context budget/workspace/profile paths**
- [x] **Step 4: Populate `RecentToolActivity` from session tool ledger with recent failures and significant successes**
- [x] **Step 5: Keep full plan/tool/result detail available through existing tools and debug surfaces**

### Task 4: Add future manageable-state follow-ups

**Files:**
- Update beads issues

- [x] **Step 1: File follow-up for `skill_list` / `skill_read` / `skill_set_status`**
- [x] **Step 2: File follow-up for `prompt_budget_read` / `prompt_budget_update`**
- [x] **Step 3: File follow-up for `autonomy_state_read` and mesh-aware expansion**
- [x] **Step 4: File follow-up for offload pin/unpin and auto-pin after 3 reads**

### Task 5: Verify and ship

**Files:**
- None

- [x] **Step 1: Run targeted tests for prompt/plan changes**
- [x] **Step 2: Run `cargo fmt --all`**
- [x] **Step 3: Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`**
- [x] **Step 4: Run `cargo test --workspace --all-features`**
- [x] **Step 5: Run `cargo build -p agentd`**
- [x] **Step 6: Run `cargo build --release -p agentd`**
- [x] **Step 7: Commit, push, and deploy to the remote server**
