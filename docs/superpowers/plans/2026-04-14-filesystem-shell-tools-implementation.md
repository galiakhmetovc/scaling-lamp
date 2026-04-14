# Filesystem and Shell Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add clean-room filesystem and shell tool domains with policy-driven exposure and execution safety on top of the existing tool pipeline.

**Architecture:** Extend the current tool architecture instead of special-casing runtime behavior. Add domain-specific contracts and executors for filesystem and shell tools, keep model-visible selection in `ToolContract`, and keep shared allow/deny in `ToolExecutionContract` before domain-specific safety enforcement.

**Tech Stack:** Go, clean-room runtime contracts, provider tool loop, YAML config, event log, projections, trash-based file deletion, bounded local command execution.

---

## File Structure

### New contract/runtime files

- Create: `internal/filesystem/executor.go`
- Create: `internal/filesystem/executor_test.go`
- Create: `internal/filesystem/definitions.go`
- Create: `internal/filesystem/definitions_test.go`
- Create: `internal/shell/executor.go`
- Create: `internal/shell/executor_test.go`
- Create: `internal/shell/definitions.go`
- Create: `internal/shell/definitions_test.go`

### Existing integration files

- Modify: `internal/contracts/contracts.go`
- Modify: `internal/policies/registry.go`
- Modify: `internal/config/registry.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/runtime/component_registry.go`
- Modify: `internal/runtime/agent_builder.go`
- Modify: `internal/tools/catalog.go`
- Modify: `internal/provider/client.go`
- Modify: `internal/runtime/tool_loop.go`

### Config files

- Create: `config/zai-smoke/contracts/filesystem-tools.yaml`
- Create: `config/zai-smoke/contracts/filesystem-execution.yaml`
- Create: `config/zai-smoke/contracts/shell-tools.yaml`
- Create: `config/zai-smoke/contracts/shell-execution.yaml`
- Create: `config/zai-smoke/policies/filesystem-tools/*.yaml`
- Create: `config/zai-smoke/policies/filesystem-execution/*.yaml`
- Create: `config/zai-smoke/policies/shell-tools/*.yaml`
- Create: `config/zai-smoke/policies/shell-execution/*.yaml`
- Modify: `config/zai-smoke/contracts/tools.yaml`
- Modify: `config/zai-smoke/contracts/tool-execution.yaml`
- Modify: `config/zai-smoke/agent.yaml`

### Docs

- Create: `docs/clean-room-filesystem-shell-tools.md`
- Modify: `docs/clean-room-current-policies-and-strategies.md`
- Modify: `docs/clean-room-current-runtime-flow.md`
- Modify: `docs/clean-room-current-system-detailed.md`

## Task 1: Add Contract Types and Policy Registry Entries

**Files:**
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/policies/registry.go`
- Modify: `internal/config/registry.go`
- Modify: `internal/runtime/contract_resolver.go`
- Test: `internal/runtime/contract_resolver_test.go`

- [ ] **Step 1: Write failing resolver tests for the four new contracts**
- [ ] **Step 2: Add contract structs and params for filesystem tool/execution and shell tool/execution**
- [ ] **Step 3: Register policy families and strategies in the policy registry**
- [ ] **Step 4: Add config kind registration and resolver paths**
- [ ] **Step 5: Run targeted resolver tests**
- [ ] **Step 6: Commit**

## Task 2: Filesystem Tool Definitions

**Files:**
- Create: `internal/filesystem/definitions.go`
- Create: `internal/filesystem/definitions_test.go`
- Modify: `internal/tools/catalog.go`
- Test: `internal/provider/client_test.go`

- [ ] **Step 1: Write failing tests for filesystem tool definitions and serialization**
- [ ] **Step 2: Implement built-in definitions for `fs_list`, `fs_read_text`, `fs_write_text`, `fs_patch_text`, `fs_mkdir`, `fs_move`, `fs_trash`**
- [ ] **Step 3: Feed filesystem definitions into the general tool catalog without coupling them to provider logic**
- [ ] **Step 4: Run targeted tests**
- [ ] **Step 5: Commit**

## Task 3: Filesystem Execution Backend

**Files:**
- Create: `internal/filesystem/executor.go`
- Create: `internal/filesystem/executor_test.go`
- Modify: `internal/runtime/tool_loop.go`

- [ ] **Step 1: Write failing tests for path scope, bounded read/write, rename, mkdir, and trash-only delete**
- [ ] **Step 2: Implement scope validation for `workspace_only` and `allowlist_paths`**
- [ ] **Step 3: Implement bounded text IO and safe mutation handling**
- [ ] **Step 4: Implement trash-backed delete instead of permanent remove**
- [ ] **Step 5: Route filesystem tool calls through the tool loop**
- [ ] **Step 6: Run targeted tests**
- [ ] **Step 7: Commit**

## Task 4: Shell Tool Definitions

**Files:**
- Create: `internal/shell/definitions.go`
- Create: `internal/shell/definitions_test.go`
- Modify: `internal/tools/catalog.go`
- Test: `internal/provider/client_test.go`

- [ ] **Step 1: Write failing tests for `shell_exec` definition and serialization**
- [ ] **Step 2: Implement built-in `shell_exec` tool definition**
- [ ] **Step 3: Keep shell descriptions and schema outside chat/provider code**
- [ ] **Step 4: Run targeted tests**
- [ ] **Step 5: Commit**

## Task 5: Shell Execution Backend

**Files:**
- Create: `internal/shell/executor.go`
- Create: `internal/shell/executor_test.go`
- Modify: `internal/runtime/tool_loop.go`

- [ ] **Step 1: Write failing tests for command allowlist, denied patterns, timeout, output limits, cwd scope, and disabled-network mode metadata**
- [ ] **Step 2: Implement allowlist-based command validation**
- [ ] **Step 3: Implement bounded non-interactive execution with structured result payload**
- [ ] **Step 4: Reject unsupported interactive or disallowed commands cleanly**
- [ ] **Step 5: Route shell tool calls through the tool loop after shared `ToolExecutionContract` gate**
- [ ] **Step 6: Run targeted tests**
- [ ] **Step 7: Commit**

## Task 6: Agent Builder and Runtime Wiring

**Files:**
- Modify: `internal/runtime/component_registry.go`
- Modify: `internal/runtime/agent_builder.go`
- Modify: `internal/provider/client.go`
- Modify: `internal/runtime/tool_loop.go`
- Test: `internal/runtime/chat_test.go`

- [ ] **Step 1: Write failing integration test where provider emits filesystem and shell tool calls**
- [ ] **Step 2: Build and inject filesystem and shell tool executors through runtime composition**
- [ ] **Step 3: Keep domain execution dispatch centralized in the tool loop**
- [ ] **Step 4: Verify tool results return to the provider loop as tool messages**
- [ ] **Step 5: Run end-to-end tests**
- [ ] **Step 6: Commit**

## Task 7: Shipped Config

**Files:**
- Create: `config/zai-smoke/contracts/filesystem-tools.yaml`
- Create: `config/zai-smoke/contracts/filesystem-execution.yaml`
- Create: `config/zai-smoke/contracts/shell-tools.yaml`
- Create: `config/zai-smoke/contracts/shell-execution.yaml`
- Create matching `policies/**/*.yaml`
- Modify: `config/zai-smoke/contracts/tools.yaml`
- Modify: `config/zai-smoke/contracts/tool-execution.yaml`
- Modify: `config/zai-smoke/agent.yaml`
- Test: `internal/runtime/agent_builder_test.go`

- [ ] **Step 1: Write failing config-loading test for filesystem and shell contract graph**
- [ ] **Step 2: Add shipped filesystem tool config with workspace-only scope and bounded IO**
- [ ] **Step 3: Add shipped shell tool config with allowlisted commands and bounded runtime**
- [ ] **Step 4: Keep plan tools alongside new domains without duplicating visibility logic**
- [ ] **Step 5: Run config/runtime tests**
- [ ] **Step 6: Commit**

## Task 8: Documentation

**Files:**
- Create: `docs/clean-room-filesystem-shell-tools.md`
- Modify: `docs/clean-room-current-policies-and-strategies.md`
- Modify: `docs/clean-room-current-runtime-flow.md`
- Modify: `docs/clean-room-current-system-detailed.md`

- [ ] **Step 1: Document the new contracts, policies, strategies, and params**
- [ ] **Step 2: Document safety boundaries and first-slice limits**
- [ ] **Step 3: Document shipped config and operator expectations**
- [ ] **Step 4: Commit**

## Task 9: Verification

**Files:**
- No new files

- [ ] **Step 1: Run focused suite**
  - `GOTMPDIR=/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/.tmp/go-build go test ./internal/contracts ./internal/policies ./internal/filesystem ./internal/shell ./internal/tools ./internal/provider ./internal/runtime -count=1`
- [ ] **Step 2: Run build**
  - `GOTMPDIR=/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/.tmp/go-build go build ./cmd/agent`
- [ ] **Step 3: Run live smoke verification if shipped config changed**
- [ ] **Step 4: Commit final verification/doc touchups if needed**

## Smell Checks

Do not accept these implementation shortcuts:

- filesystem or shell tool execution hardcoded inside `chat.go`
- domain-specific execution bypassing `ToolContract`
- domain-specific execution bypassing `ToolExecutionContract`
- permanent delete implementation
- unrestricted absolute path access by default
- arbitrary shell command execution without command policy validation
- interactive PTY behavior hidden behind `shell_exec`
- prompt assembly reading tool contracts directly

## Follow-Up

Expected later follow-up, not in this plan:

- shell output streaming
- human approval UX
- richer patch application format
- binary file/artifact support
- provider trace capture for filesystem/shell tool results
