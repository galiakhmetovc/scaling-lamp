# Agent Browser Browserless Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a production browser automation path using agent-browser and Browserless without creating a second teamD runtime path.

**Architecture:** `browser_*` tools are first-class built-in tools in `agent-runtime`, executed by `ToolRuntime` through an `agent-browser` CLI adapter. `cmd/agentd` passes session-scoped browser config into the runtime so every teamD session gets isolated browser state. Browserless is installed by the container deploy script as local Chromium infrastructure.

**Tech Stack:** Rust, agent-browser CLI, Browserless Docker image `ghcr.io/browserless/chromium`, existing teamD provider loop/tool ledger/artifact offload.

---

### Task 1: Runtime Browser Tool Surface

**Files:**

- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/names.rs`
- Modify: `crates/agent-runtime/src/tool/inputs.rs`
- Modify: `crates/agent-runtime/src/tool/schema.rs`
- Modify: `crates/agent-runtime/src/tool/catalog.rs`
- Modify: `crates/agent-runtime/src/tool/parse.rs`
- Modify: `crates/agent-runtime/src/tool/outputs.rs`
- Create: `crates/agent-runtime/src/tool/browser.rs`
- Test: `crates/agent-runtime/src/tool/tests.rs`

- [ ] Add failing parser/schema tests for `browser_open`, `browser_snapshot`, and `browser_screenshot`.
- [ ] Add `ToolFamily::Browser` and `ToolName::Browser*` variants.
- [ ] Add typed input structs and `ToolCall` variants.
- [ ] Add JSON schemas with explicit enums and no additional properties.
- [ ] Add catalog definitions and include browser tools in automatic model definitions only when enabled by profile/config.
- [ ] Add typed outputs, summaries, and model JSON outputs.
- [ ] Implement `BrowserToolClient` with command timeout, sanitized session env, Browserless env, and safe argument vectors.
- [ ] Wire `ToolRuntime` to invoke browser tools through `BrowserToolClient`.
- [ ] Run targeted runtime tests.

### Task 2: Config and Execution Wiring

**Files:**

- Modify: `crates/agent-persistence/src/config.rs`
- Modify: `crates/agent-persistence/src/config/tests.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/execution/provider_offload.rs`
- Modify: `config.example.toml`

- [ ] Add failing config tests for `[browser]`, `[browser.browserless]`, and env overrides.
- [ ] Add `BrowserConfig` to `AppConfig` and map it to `agent_runtime::tool::BrowserToolConfig`.
- [ ] Pass current `session_id` into `ToolRuntime` so `AGENT_BROWSER_SESSION` is isolated.
- [ ] Offload large browser text/snapshot/eval outputs through existing offload path.
- [ ] Keep all tool calls in the existing provider loop and ledger.
- [ ] Run targeted config/execution tests.

### Task 3: Built-in Skills and Prompt Guidance

**Files:**

- Modify: `cmd/agentd/src/agents.rs`
- Modify: `docs/current/15-tool-reference.md`
- Modify: `docs/current/01-architecture.md`

- [ ] Add/replace built-in `agent-browser` skill.
- [ ] Keep Lightpanda documented as legacy/beta fallback.
- [ ] Document the browser snapshot-ref-act loop and when to prefer browser tools over `web_fetch`.
- [ ] Add tests asserting the skill is installed in default agent home.

### Task 4: Deploy Script and Container Add-on

**Files:**

- Modify: `scripts/deploy-teamd-containers.sh`
- Modify: `scripts/test-deploy-teamd.sh`
- Modify: `docs/current/14-container-addons.md`
- Modify: `docs/current/09-operator-cheatsheet.md`

- [ ] Add dry-run tests for `--with-browserless` and `--with-agent-browser`.
- [ ] Install `agent-browser` into `/opt/teamd/bin/agent-browser`.
- [ ] Add Browserless env generation and Docker Compose service.
- [ ] Upsert `[browser]` and `[browser.browserless]` config blocks.
- [ ] Keep Browserless bound to localhost by default.
- [ ] Document install, update, smoke checks, and rollback.

### Task 5: Verification and Release Prep

**Files:**

- Modify as needed from previous tasks.

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test -p agent-runtime`.
- [ ] Run `cargo test -p agent-persistence`.
- [ ] Run `./scripts/test-deploy-teamd.sh`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run `cargo test --workspace --all-features`.
- [ ] Run `cargo build -p agentd`.
- [ ] Run `cargo build --release -p agentd`.
- [ ] Commit and push.
