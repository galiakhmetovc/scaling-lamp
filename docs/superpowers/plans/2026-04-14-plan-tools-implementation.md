# Plan Tools Domain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an internal event-sourced planning domain with plan-management tools exposed to the agent through the clean-room tool system and projected into session head.

**Architecture:** Build a new plan domain on top of the existing event log and projection store. Keep plan commands, plan projections, tool exposure, and session-head rendering as separate seams. Do not hardcode plan management in chat runtime.

**Tech Stack:** Go, clean-room runtime contracts, event log, projections, tool catalog/execution gate, YAML config.

---

## File Structure

### New runtime/domain files

- Create: `internal/runtime/plans/events.go`
- Create: `internal/runtime/plans/service.go`
- Create: `internal/runtime/plans/service_test.go`
- Create: `internal/runtime/projections/active_plan.go`
- Create: `internal/runtime/projections/active_plan_test.go`
- Create: `internal/runtime/projections/plan_archive.go`
- Create: `internal/runtime/projections/plan_archive_test.go`
- Create: `internal/runtime/projections/plan_head.go`
- Create: `internal/runtime/projections/plan_head_test.go`
- Create: `internal/tools/plan_tools.go`
- Create: `internal/tools/plan_tools_test.go`

### Existing runtime integration files

- Modify: `internal/runtime/eventing/events.go`
- Modify: `internal/runtime/projections/registry.go`
- Modify: `internal/runtime/chat.go`
- Modify: `internal/promptassembly/executor.go`
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/policies/registry.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/runtime/component_registry.go`
- Modify: `internal/runtime/agent_builder.go`
- Modify: `internal/provider/client.go`

### Config files

- Create: `config/zai-smoke/contracts/plan-tools.yaml`
- Create: `config/zai-smoke/policies/tools/plan-tools-catalog.yaml`
- Modify: `config/zai-smoke/contracts/tools.yaml`
- Modify: `config/zai-smoke/contracts/tool-execution.yaml`
- Modify: `config/zai-smoke/contracts/prompt-assembly.yaml`
- Modify: `config/zai-smoke/agent.yaml`

### Docs

- Modify: `docs/clean-room-current-policies-and-strategies.md`
- Modify: `docs/clean-room-current-runtime-flow.md`
- Modify: `docs/clean-room-current-system-detailed.md`
- Create: `docs/clean-room-plan-tools.md`

## Task 1: Plan Event Model

**Files:**
- Create: `internal/runtime/plans/events.go`
- Modify: `internal/runtime/eventing/events.go`
- Test: `internal/runtime/plans/service_test.go`

- [ ] **Step 1: Write failing tests for plan events and transitions**
- [ ] **Step 2: Add typed plan/task structs and event payload helpers**
- [ ] **Step 3: Extend global event-kind catalog with plan events**
- [ ] **Step 4: Run targeted tests**
- [ ] **Step 5: Commit**

## Task 2: Plan Command Service

**Files:**
- Create: `internal/runtime/plans/service.go`
- Create: `internal/runtime/plans/service_test.go`

- [ ] **Step 1: Write failing tests for `init_plan`, `add_task`, `set_task_status`, `add_task_note`, `edit_task`, and dependency-aware validation**
- [ ] **Step 2: Implement service that emits plan events without touching chat code**
- [ ] **Step 3: Enforce single-active-plan archive behavior**
- [ ] **Step 4: Enforce task transition rules and dependency rules**
- [ ] **Step 5: Run targeted tests**
- [ ] **Step 6: Commit**

## Task 3: Plan Projections

**Files:**
- Create: `internal/runtime/projections/active_plan.go`
- Create: `internal/runtime/projections/plan_archive.go`
- Create: `internal/runtime/projections/plan_head.go`
- Create matching `*_test.go`
- Modify: `internal/runtime/projections/registry.go`

- [ ] **Step 1: Write failing projection tests for active, archive, and head views including computed `ready` state**
- [ ] **Step 2: Implement `ActivePlanProjection`**
- [ ] **Step 3: Implement `PlanArchiveProjection`**
- [ ] **Step 4: Implement `PlanHeadProjection` with compact formatting inputs and computed dependency-aware `ready` view**
- [ ] **Step 5: Register projections**
- [ ] **Step 6: Run projection tests**
- [ ] **Step 7: Commit**

## Task 4: Prompt Assembly Reads Plan Head

**Files:**
- Modify: `internal/promptassembly/executor.go`
- Modify: `internal/runtime/chat.go`
- Test: `internal/promptassembly/executor_test.go`

- [ ] **Step 1: Write failing tests showing session head includes compact plan state**
- [ ] **Step 2: Add plan-head projection input to prompt assembly**
- [ ] **Step 3: Keep transcript/session summary and plan summary boundaries explicit**
- [ ] **Step 4: Run targeted tests**
- [ ] **Step 5: Commit**

## Task 5: Plan Tools Runtime Surface

**Files:**
- Create: `internal/tools/plan_tools.go`
- Create: `internal/tools/plan_tools_test.go`
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/policies/registry.go`

- [ ] **Step 1: Write failing tests for plan tool definitions**
- [ ] **Step 2: Define `PlanToolContract` and any required policy families**
- [ ] **Step 3: Implement runtime plan tool definitions for the five tools**
- [ ] **Step 4: Run targeted tests**
- [ ] **Step 5: Commit**

## Task 6: Expose Plan Tools Through ToolContract

**Files:**
- Modify: `internal/tools/catalog.go`
- Modify: `internal/provider/client.go`
- Modify: `internal/runtime/component_registry.go`
- Modify: `internal/runtime/agent_builder.go`
- Test: `internal/provider/client_test.go`

- [ ] **Step 1: Write failing integration test showing plan tools appear in outbound request body**
- [ ] **Step 2: Feed plan tool definitions into the general tool catalog**
- [ ] **Step 3: Keep plan tool definitions separate from transport/request-shape logic**
- [ ] **Step 4: Run targeted tests**
- [ ] **Step 5: Commit**

## Task 7: Execute Plan Tool Calls

**Files:**
- Modify: `internal/provider/client.go`
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/smoke.go`
- Modify: `internal/runtime/plans/service.go`
- Test: `internal/runtime/chat_test.go`

- [ ] **Step 1: Write failing end-to-end test where provider emits a plan tool call**
- [ ] **Step 2: Route plan tool calls through `ToolExecutionContract`**
- [ ] **Step 3: Execute allowed plan tool calls via plan service**
- [ ] **Step 4: Append resulting plan events and return tool-result messages to conversation loop**
- [ ] **Step 5: Run end-to-end tests**
- [ ] **Step 6: Commit**

## Task 8: Shipped Config

**Files:**
- Create: `config/zai-smoke/contracts/plan-tools.yaml`
- Create: `config/zai-smoke/policies/tools/plan-tools-catalog.yaml`
- Modify: `config/zai-smoke/contracts/tools.yaml`
- Modify: `config/zai-smoke/contracts/tool-execution.yaml`
- Modify: `config/zai-smoke/contracts/prompt-assembly.yaml`
- Modify: `config/zai-smoke/agent.yaml`

- [ ] **Step 1: Write config-loading test for new plan-tools contract graph**
- [ ] **Step 2: Add plan-tools contract and policies**
- [ ] **Step 3: Expose plan tools in shipped config**
- [ ] **Step 4: Keep tool-execution safety explicit**
- [ ] **Step 5: Run config/runtime tests**
- [ ] **Step 6: Commit**

## Task 9: Documentation

**Files:**
- Create: `docs/clean-room-plan-tools.md`
- Modify: `docs/clean-room-current-policies-and-strategies.md`
- Modify: `docs/clean-room-current-runtime-flow.md`
- Modify: `docs/clean-room-current-system-detailed.md`

- [ ] **Step 1: Document the new plan domain and tool exposure path**
- [ ] **Step 2: Document single-active-plan archive rule**
- [ ] **Step 3: Document current limitations and future `bd` sync**
- [ ] **Step 4: Commit**

## Task 10: Verification

**Files:**
- No new files

- [ ] **Step 1: Run full test suite**
  - `GOTMPDIR=/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/.tmp/go-build go test ./internal/config ./internal/contracts ./internal/policies ./internal/promptassembly ./internal/tools ./internal/provider ./internal/runtime ./internal/runtime/projections ./cmd/agent -count=1`
- [ ] **Step 2: Run build**
  - `GOTMPDIR=/home/admin/AI-AGENT/data/projects/teamD/.worktrees/rewrite-clean-room-root/.tmp/go-build go build ./cmd/agent`
- [ ] **Step 3: Run live smoke verification if config changed**
- [ ] **Step 4: Commit final verification/doc touchups if needed**

## Smell Checks

Do not accept these implementation shortcuts:

- plan state assembled directly inside `chat.go`
- plan summary built directly from raw event scans in prompt assembly
- plan tools hardcoded inside provider client without contract/tool catalog path
- plan commands mutating snapshots directly without events
- using `delete_task`
- exposing `get_plan` as a tool instead of projecting plan into session head
- storing `ready` as a persisted task status instead of computing it in projection

## Follow-Up

Expected later follow-up, not in this plan:

- sync internal plan domain with `bd`
- terminal commands for explicit plan inspection/editing
- richer archive browsing
