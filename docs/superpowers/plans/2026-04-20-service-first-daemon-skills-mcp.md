# Service-First Daemon, Skills, and MCP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `agentd` into a daemon-first service with HTTP/JSON transport, daemon-backed TUI/CLI clients, hosted background jobs and schedules, and agentskills-compatible skill discovery and activation. Prepare MCP as the next implementation wave on top of that service foundation.

**Architecture:** The daemon becomes the source of truth for sessions, runs, process registries, prompt assembly, background jobs, schedules, and skills. TUI and CLI become thin clients over HTTP/JSON, with local auto-spawn for convenience. Skills join the canonical prompt path through daemon-owned activation and progressive disclosure rather than a side channel.

**Tech Stack:** Rust, current `agentd`/`agent-runtime`/`agent-persistence` workspace, HTTP/JSON server/client stack in Rust, existing SQLite persistence, existing TUI.

---

## File Structure

### New/expanded daemon transport surface

- Create: `cmd/agentd/src/daemon.rs`
  - daemon entrypoint and lifecycle orchestration
- Create: `cmd/agentd/src/http/`
  - HTTP server routing, auth, DTOs, and client transport
- Create: `cmd/agentd/src/http/server.rs`
  - daemon listener, request routing, bearer enforcement
- Create: `cmd/agentd/src/http/client.rs`
  - TUI/CLI daemon client
- Create: `cmd/agentd/src/http/types.rs`
  - stable JSON request/response types

### App/runtime changes

- Modify: `cmd/agentd/src/bootstrap.rs`
  - app config/runtime wiring for daemon mode and shared service state
- Modify: `cmd/agentd/src/cli.rs`
  - `daemon` command, remote flags, Russian command aliases
- Modify: `cmd/agentd/src/tui.rs`
  - daemon client mode and local auto-spawn connection flow
- Modify: `cmd/agentd/src/execution.rs`
  - daemon-hosted execution service boundaries

### Background and schedule hosting

- Modify: `cmd/agentd/src/execution/supervisor.rs`
- Modify: `cmd/agentd/src/execution/mission.rs`
- Modify: `cmd/agentd/src/execution/chat.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
  - ensure daemon-owned execution paths stay canonical

### Skills

- Create: `crates/agent-runtime/src/skills.rs`
  - skills catalog, parsed metadata, activation model
- Create: `crates/agent-runtime/src/skills/parser.rs`
  - `SKILL.md` frontmatter/body parsing
- Create: `crates/agent-runtime/src/skills/catalog.rs`
  - directory scan and catalog load
- Create: `crates/agent-runtime/src/skills/activation.rs`
  - per-session activation and override model
- Modify: `crates/agent-runtime/src/prompt.rs`
  - active skills in prompt order
- Modify: `cmd/agentd/src/prompting.rs`
  - daemon-owned skill prompt loading

### Config

- Modify: `crates/agent-persistence/src/config.rs`
  - daemon host/port/token/skills_dir bindings

### Tests

- Create: `cmd/agentd/tests/daemon_http.rs`
- Create: `cmd/agentd/tests/daemon_tui.rs`
- Create: `crates/agent-runtime/tests/skills_catalog.rs`
- Create: `crates/agent-runtime/tests/skills_prompt.rs`

## Phase A: Daemon HTTP Foundation (`teamD-daemon.1`)

- [ ] **Step 1: Write failing config tests for daemon settings**
  - Add tests in `crates/agent-persistence/src/config.rs` for:
    - `daemon.bind_host`
    - `daemon.bind_port`
    - `daemon.bearer_token`
    - `daemon.skills_dir`
  - Verify defaults and env overrides fail before implementation.

- [ ] **Step 2: Run the targeted config tests to verify they fail**
  - Run: `cargo test -p agent-persistence daemon_ -- --nocapture`
  - Expected: missing fields or parsing failures for the new daemon config surface.

- [ ] **Step 3: Implement daemon config loading**
  - Add daemon settings to `AppConfig`.
  - Wire file/env parsing and validation.
  - Keep default bind safe for local-first usage.

- [ ] **Step 4: Run config tests to verify they pass**
  - Run: `cargo test -p agent-persistence daemon_ -- --nocapture`
  - Expected: all new daemon config tests pass.

- [ ] **Step 5: Write failing transport tests**
  - Add `cmd/agentd/tests/daemon_http.rs` covering:
    - `/v1/status`
    - bearer auth required when configured
    - request/response JSON shape for one simple session call

- [ ] **Step 6: Run the daemon HTTP tests to verify they fail**
  - Run: `cargo test -p agentd daemon_http -- --nocapture`
  - Expected: missing daemon server/client implementation.

- [ ] **Step 7: Implement daemon entrypoint and HTTP server**
  - Add `agentd daemon`
  - Add HTTP server wiring and JSON DTOs
  - Add bearer token check
  - Keep daemon runtime thin; route into existing app/runtime methods

- [ ] **Step 8: Run daemon HTTP tests to verify they pass**
  - Run: `cargo test -p agentd daemon_http -- --nocapture`
  - Expected: green.

- [ ] **Step 9: Commit**
  - Commit message: `feat: add daemon http control plane`

## Phase B: Daemon Client and Local Auto-Spawn (`teamD-daemon.2`)

- [ ] **Step 1: Write failing TUI/CLI daemon client tests**
  - Add tests in `cmd/agentd/tests/daemon_tui.rs` for:
    - TUI connecting to a running daemon
    - local auto-spawn when the daemon is absent
    - explicit remote host/port bypassing auto-spawn

- [ ] **Step 2: Run the client-mode tests to verify they fail**
  - Run: `cargo test -p agentd daemon_tui -- --nocapture`
  - Expected: no daemon client/auto-spawn support yet.

- [ ] **Step 3: Implement daemon client transport**
  - Add client wrappers in `cmd/agentd/src/http/client.rs`
  - Route session/chat/plan/status calls through HTTP

- [ ] **Step 4: Implement local auto-spawn**
  - In `agentd tui` and daemon-backed CLI flows:
    - attempt connection first
    - if target is local and unavailable, spawn local daemon and retry

- [ ] **Step 5: Add explicit remote flags**
  - Support `--host` and `--port`
  - Ensure remote targets do not auto-spawn local daemon

- [ ] **Step 6: Run daemon client tests to verify they pass**
  - Run: `cargo test -p agentd daemon_tui -- --nocapture`

- [ ] **Step 7: Commit**
  - Commit message: `feat: add daemon client mode and autospawn`

## Phase C: Background Jobs and Schedules Hosted by the Daemon (`teamD-bg.*`, `teamD-cron.*`, `teamD-daemon.3`, `teamD-daemon.4`)

- [ ] **Step 1: Complete durable background job model and persistence**
  - Implement `teamD-bg.1`
  - Keep background jobs in the canonical store model

- [ ] **Step 2: Add failing tests for daemon-hosted worker loop**
  - Add tests covering:
    - long-running job pickup
    - progress/log persistence
    - cancel/recovery

- [ ] **Step 3: Implement background worker loop**
  - Implement `teamD-bg.2`
  - Host it inside the daemon process

- [ ] **Step 4: Add failing schedule tests**
  - Add tests for:
    - next-run calculation
    - missed-run policy
    - dispatch into background jobs

- [ ] **Step 5: Implement recurring schedules**
  - Implement `teamD-cron.1` and `teamD-cron.2`
  - Ensure schedules dispatch jobs through the same background runtime

- [ ] **Step 6: Expose daemon-backed background/schedule controls in client surfaces**
  - Implement `teamD-bg.3`, `teamD-cron.3`, and `teamD-daemon.4`
  - TUI and CLI must read state from the daemon, not from local side effects

- [ ] **Step 7: Run integrated background/schedule tests**
  - Run: `cargo test -p agentd background -- --nocapture`
  - Run: `cargo test -p agentd schedule -- --nocapture`
  - Expand names as real tests land.

- [ ] **Step 8: Commit**
  - Commit message: `feat: host background jobs and schedules in daemon`

## Phase D: Skills Discovery and Activation (`teamD-skills.1`, `teamD-skills.2`)

- [ ] **Step 1: Write failing skill catalog tests**
  - Add `crates/agent-runtime/tests/skills_catalog.rs` covering:
    - scan `skills/*/SKILL.md`
    - parse `name` and `description`
    - malformed frontmatter handling
    - no eager load of full resources

- [ ] **Step 2: Run the skill catalog tests to verify they fail**
  - Run: `cargo test -p agent-runtime skills_catalog -- --nocapture`

- [ ] **Step 3: Implement agentskills-compatible scan and parse**
  - Add parser and catalog modules
  - Keep only `name + description` in the daemon catalog at startup

- [ ] **Step 4: Write failing session activation tests**
  - Cover:
    - auto activation by description match
    - manual `\включить <skill>`
    - manual `\выключить <skill>`
    - activation scoped only to current session

- [ ] **Step 5: Implement per-session skill activation**
  - Add daemon-owned activation state
  - Add Russian commands and slash aliases

- [ ] **Step 6: Run catalog and activation tests to verify they pass**
  - Run: `cargo test -p agent-runtime skills_ -- --nocapture`
  - Run: `cargo test -p agentd skills_ -- --nocapture`

- [ ] **Step 7: Commit**
  - Commit message: `feat: add daemon skill catalog and activation`

## Phase E: Skill Prompt Integration and Visibility (`teamD-skills.3`, `teamD-skills.4`)

- [ ] **Step 1: Write failing prompt-assembly tests for active skills**
  - Add `crates/agent-runtime/tests/skills_prompt.rs`
  - Verify active skills appear between `AGENTS.md` and `SessionHead`
  - Verify inactive skills do not load

- [ ] **Step 2: Run the prompt tests to verify they fail**
  - Run: `cargo test -p agent-runtime skills_prompt -- --nocapture`

- [ ] **Step 3: Implement progressive disclosure in prompt assembly**
  - Load full `SKILL.md` only when active
  - Load skill references/scripts/assets only on demand from active instructions

- [ ] **Step 4: Add failing TUI/CLI visibility tests**
  - Cover `\скиллы`
  - Cover active skill display
  - Cover daemon-backed status rendering

- [ ] **Step 5: Implement operator visibility**
  - Expose catalog and active state in TUI/CLI
  - Keep daemon as source of truth

- [ ] **Step 6: Run prompt and visibility tests to verify they pass**
  - Run: `cargo test -p agent-runtime skills_prompt -- --nocapture`
  - Run: `cargo test -p agentd skills_visibility -- --nocapture`

- [ ] **Step 7: Commit**
  - Commit message: `feat: integrate active skills into daemon prompt path`

## Phase F: MCP Follow-Up Wave (`teamD-mcp.*`)

- [ ] **Step 1: Write failing daemon MCP configuration and connector tests**
  - Add tests for connector lifecycle and config parsing.

- [ ] **Step 2: Run MCP config tests to verify they fail**
  - Run: `cargo test -p agentd mcp_ -- --nocapture`

- [ ] **Step 3: Implement daemon-managed MCP connector lifecycle**
  - Implement `teamD-mcp.1`

- [ ] **Step 4: Write failing canonical MCP runtime tests**
  - Cover MCP tools/resources/prompts joining the existing runtime path.

- [ ] **Step 5: Implement MCP-backed runtime integration**
  - Implement `teamD-mcp.2`

- [ ] **Step 6: Expose MCP status and controls in TUI/CLI**
  - Implement `teamD-mcp.3`

- [ ] **Step 7: Run MCP tests to verify they pass**
  - Run: `cargo test -p agentd mcp_ -- --nocapture`

- [ ] **Step 8: Commit**
  - Commit message: `feat: add daemon mcp client support`

## Final Verification

- [ ] Run formatting and lint:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`

- [ ] Run the full test suite:
  - `cargo test --workspace --all-features`

- [ ] Build Linux release:
  - `cargo build --release -p agentd`

- [ ] Build Windows release:
  - `cargo build --release -p agentd --target x86_64-pc-windows-gnu`

- [ ] Update distribution artifacts if needed

- [ ] Commit final integration work

