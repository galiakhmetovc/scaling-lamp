# Offload Retrieval Prompt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose offloaded context references in the canonical prompt and let the model recover full payloads only through explicit retrieval tools.

**Architecture:** Keep the single runtime path. Prompt assembly gains a compact offload refs block, while `artifact_read` and `artifact_search` execute through the same model-driven tool loop as `plan_*` tools.

**Tech Stack:** Rust, SQLite-backed persistence, current provider loop, canonical tool catalog.

---

### Task 1: Add failing prompt and retrieval tests

**Files:**
- Modify: `crates/agent-runtime/src/prompt.rs`
- Modify: `cmd/agentd/tests/bootstrap_app.rs`

- [ ] Add a failing prompt assembly test that expects offload refs to render as a dedicated system message.
- [ ] Run the focused prompt test and verify it fails for missing offload prompt support.
- [ ] Add a failing integration test that drives `artifact_read` through the existing provider loop.
- [ ] Run the focused integration test and verify it fails because retrieval tools do not exist yet.

### Task 2: Add canonical offload retrieval tool surface

**Files:**
- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/tests.rs`
- Modify: `crates/agent-runtime/src/permission.rs`

- [ ] Add `artifact_read` and `artifact_search` tool names, schemas, outputs, summaries, and model output rendering.
- [ ] Keep them read-only and non-approval tools.
- [ ] Add/adjust catalog tests so the new tools appear in the canonical automatic tool surface.
- [ ] Run the targeted runtime tests and make them pass.

### Task 3: Integrate offload refs into prompt assembly and provider loop

**Files:**
- Modify: `crates/agent-runtime/src/prompt.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`

- [ ] Load `ContextOffloadSnapshot` alongside summary and plan when assembling prompt messages.
- [ ] Render compact offload refs after context summary and before uncovered transcript tail.
- [ ] Only expose retrieval tools to the provider when the session has offload refs.
- [ ] Run focused prompt/provider tests and make them pass.

### Task 4: Execute retrieval tools through the canonical session path

**Files:**
- Modify: `cmd/agentd/src/execution/provider_loop.rs`

- [ ] Implement `artifact_read` and `artifact_search` in `execute_model_tool_call`.
- [ ] Keep retrieval session-scoped by resolving refs from the current session's offload snapshot.
- [ ] Return structured tool outputs so continuation requests can use them immediately.
- [ ] Run focused integration tests and make them pass.

### Task 5: Verify and land

**Files:**
- Modify as needed based on prior tasks

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace --all-features`.
- [ ] Update and close `teamD-offload.2`.
- [ ] Commit with a focused message and push the branch.
