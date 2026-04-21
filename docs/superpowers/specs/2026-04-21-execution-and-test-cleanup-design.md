# Execution And Test Cleanup Design

## Goal

Reduce maintenance pressure before the next feature wave by tightening two hotspots:

1. split `cmd/agentd/src/execution/chat.rs` along already-visible seams;
2. move scenario-heavy inline tests out of hot runtime/persistence source files.

This is a behavior-preserving cleanup pass. No second runtime path, no prompt/tool-loop changes.

## Scope

### In

- extract delegate job execution and session wake-up handling from `execution/chat.rs` into focused execution modules;
- keep canonical execution flow unchanged;
- move multi-case contract tests out of:
  - `crates/agent-persistence/src/config.rs`
  - `crates/agent-persistence/src/records.rs`
  - `crates/agent-runtime/src/scheduler.rs`
- leave small local unit tests in place when they are tightly coupled and cheap to read.

### Out

- no broad runtime/provider/tool surface redesign;
- no MCP/background/cron feature changes;
- no prompt order changes;
- no daemon/TUI transport behavior changes.

## Target Structure

### Execution

- `cmd/agentd/src/execution/chat.rs`
  - keep foreground chat-turn logic only
- `cmd/agentd/src/execution/delegate_jobs.rs`
  - local/remote delegate background execution helpers
- `cmd/agentd/src/execution/wakeup.rs`
  - inbox-driven wake-up turn helpers

### Tests

- `crates/agent-persistence/src/config/tests.rs`
- `crates/agent-persistence/src/records/tests.rs`
- `crates/agent-runtime/src/scheduler/tests.rs`

## Constraints

- preserve one canonical runtime path;
- preserve existing A2A/delegation/background behavior;
- keep tests authoritative: no test deletion without equal or stronger coverage elsewhere.
