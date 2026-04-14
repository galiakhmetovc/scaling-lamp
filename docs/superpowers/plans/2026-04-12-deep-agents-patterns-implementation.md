# Deep Agents Patterns Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add artifact offload, persistent plans, and worker handoff contracts to `teamD` so long-running runs stay inspectable and prompt-efficient.

**Architecture:** Extend the existing transport-agnostic runtime instead of introducing a new framework layer. Implement the three Deep Agents-inspired patterns in dependency order: first artifact offload in the tool/result path, then persisted plan state over runs/workers, then structured worker handoff that depends on artifacts and plan semantics.

**Tech Stack:** Go, existing `internal/runtime`, `internal/api`, `internal/cli`, `internal/artifacts`, SQLite/Postgres runtime stores, Go tests.

---

## File Map

### Existing files to modify

- `internal/runtime/types.go`
  - Add artifact-offload, plan, and worker-handoff domain types.
- `internal/runtime/store.go`
  - Extend store interfaces for plans and handoffs.
- `internal/runtime/sqlite_store.go`
  - Add schema and persistence for plan and handoff data.
- `internal/runtime/postgres_store.go`
  - Add schema and persistence for plan and handoff data.
- `internal/runtime/execution_service.go`
  - Keep runtime-owned execution semantics while integrating new offload/handoff hooks.
- `internal/runtime/runtime_api.go`
  - Add runtime queries and mutations for plans and handoffs.
- `internal/api/server.go`
  - Expose plans, handoffs, and artifact reads through HTTP.
- `internal/api/types.go`
  - Add DTOs for plans and handoffs.
- `internal/cli/client.go`
  - Add client methods for plans/handoffs/artifact reads.
- `cmd/coordinator/cli.go`
  - Add `plans` and `artifacts` commands and render helpers.
- `internal/transport/telegram/provider_tools.go`
  - Consume runtime-owned shaped tool results instead of enforcing offload policy locally.
- `internal/transport/telegram/tool_helpers.go`
  - Shared helpers for transcript-safe tool result payloads.
- `internal/runtime/workers_service.go`
  - Create and persist worker handoffs.
- `internal/runtime/jobs_service.go`
  - Use the same artifact offload policy for large command outputs where applicable.
- `docs/agent/04-tool-loop.md`
  - Explain artifact offload in the tool loop.
- `docs/agent/http-api.md`
  - Document plans/artifacts/handoffs endpoints.
- `docs/agent/cli.md`
  - Document plan/artifact commands.
- `docs/agent/workers.md`
  - Explain worker handoff contract.
- `docs/agent/06-compaction.md`
  - Explain how artifact refs improve compaction quality.

### New files to create

- `internal/runtime/artifact_offload.go`
  - Offload policy, preview generation, and transcript payload shaping.
- `internal/runtime/tool_results.go`
  - Transport-agnostic tool output shaping and artifact-ref payloads.
- `internal/runtime/plans_service.go`
  - Runtime-owned plan CRUD/update logic.
- `internal/runtime/handoffs_service.go`
  - Runtime-owned worker handoff shaping and reads.
- `internal/runtime/plans_service_test.go`
  - Tests for persistent plan state and transitions.
- `internal/runtime/artifact_offload_test.go`
  - Tests for large output offload behavior.
- `internal/runtime/handoffs_service_test.go`
  - Tests for worker handoff creation and reads.
- `docs/agent/plans.md`
  - Beginner guide for persisted plans.
- `docs/agent/artifact-offload.md`
  - Beginner guide for offloaded tool outputs.

## Phase Order

- Phase 1: Artifact offload
- Phase 2: Persistent plan state
- Phase 3: Worker handoff contract

The order is mandatory because:

- plans and handoffs should reference artifact refs once that mechanism exists
- worker handoff should return artifact refs instead of large raw payloads

### Task 1: Artifact Offload Persistence, Domain, And Tests

**Files:**
- Create: `internal/runtime/artifact_offload.go`
- Create: `internal/runtime/artifact_offload_test.go`
- Create: `internal/runtime/tool_results.go`
- Modify: `internal/runtime/types.go`
- Modify: `internal/artifacts/store.go`
- Modify: `internal/runtime/execution_service.go`
- Modify: `internal/transport/telegram/provider_tools.go`
- Modify: `internal/transport/telegram/tool_helpers.go`
- Test: `internal/api/server_test.go`
- Test: `internal/cli/client_test.go`

- [ ] **Step 1: Write failing tests for offload policy**

Add tests that cover:
- small tool output stays inline
- large tool output is replaced by preview + `artifact_ref`
- preview lines are bounded
- disabled tools are not offloaded
- full output is persisted before `artifact_ref` is emitted
- artifact reads return the full stored payload

- [ ] **Step 2: Run artifact-offload tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/api ./internal/cli -run 'Artifact|Offload' -count=1`
Expected: FAIL because offload types/helpers do not exist yet.

- [ ] **Step 3: Add minimal domain types**

Add exact types in `internal/runtime/types.go`:
- `ArtifactOffloadPolicy`
- `OffloadedToolResult`
- `ArtifactOwnerRef`
- `ToolResultEnvelope`

- [ ] **Step 4: Add artifact persistence support needed by offload**

Expose the exact write/read surface needed for large tool-output persistence in `internal/artifacts/store.go`.

- [ ] **Step 5: Implement minimal offload helper**

Create `internal/runtime/artifact_offload.go` with:
- offload decision by char/line thresholds
- preview extraction
- transcript-safe payload generation

- [ ] **Step 6: Add transport-agnostic tool result shaping**

Create `internal/runtime/tool_results.go` so artifact offload happens in runtime-owned logic, not directly inside Telegram transport.

- [ ] **Step 7: Wire tool result shaping**

Modify the execution/tool-result path to call runtime-owned shaping before transcript/prompt insertion. Telegram should only consume the shaped result.

- [ ] **Step 8: Add artifact read endpoints and CLI reads**

Implement:
- `GET /api/artifacts/{ref}`
- `GET /api/artifacts/{ref}/content`
- `teamd-agent artifacts show <ref>`
- `teamd-agent artifacts cat <ref>`

- [ ] **Step 9: Emit artifact-offload events and response fields**

Add event emission and DTO propagation for:
- `artifact.offloaded`
- artifact refs in run/job/worker visible responses where relevant

- [ ] **Step 10: Add targeted tests for artifact events and DTO propagation**

Assert:
- `artifact.offloaded` is persisted in the runtime event plane
- API responses expose `artifact_ref`
- CLI client can read the propagated ref/metadata

- [ ] **Step 11: Re-run artifact-offload tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/api ./internal/cli -run 'Artifact|Offload' -count=1`
Expected: PASS

- [ ] **Step 12: Commit**

```bash
git add internal/runtime/types.go internal/runtime/artifact_offload.go internal/runtime/tool_results.go internal/runtime/artifact_offload_test.go internal/artifacts/store.go internal/runtime/execution_service.go internal/transport/telegram/provider_tools.go internal/transport/telegram/tool_helpers.go internal/api/server.go internal/api/types.go internal/cli/client.go cmd/coordinator/cli.go internal/api/server_test.go internal/cli/client_test.go
git commit -m "feat(teamD): add artifact offload policy for large tool outputs"
```

### Task 2: Persistent Plan State

**Files:**
- Create: `internal/runtime/plans_service.go`
- Create: `internal/runtime/plans_service_test.go`
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/runtime/postgres_store.go`
- Modify: `internal/runtime/runtime_api.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/types.go`
- Modify: `internal/cli/client.go`
- Modify: `cmd/coordinator/cli.go`

- [ ] **Step 1: Write failing runtime tests for plan lifecycle**

Cover:
- create plan
- replace items
- append note
- start item
- complete item
- list by owner

- [ ] **Step 2: Run plan tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run TestPlansService -count=1`
Expected: FAIL because plan service/store schema do not exist.

- [ ] **Step 3: Add domain types and store interfaces**

Add:
- `PlanRecord`
- `PlanItem`
- `PlanStatus`
- `PlanQuery`

- [ ] **Step 4: Implement runtime plan service**

Create `plans_service.go` with the exact runtime methods needed by API/CLI.

- [ ] **Step 5: Add SQLite/Postgres persistence**

Add tables:
- `runtime_plans`
- `runtime_plan_items`

Add read/write methods to both backends.

- [ ] **Step 6: Expose plan API and CLI**

Add:
- `GET /api/plans`
- `GET /api/plans/{id}`
- `POST /api/plans`
- `POST /api/plans/{id}/items`
- `PUT /api/plans/{id}/items`
- `POST /api/plans/{id}/notes`
- `POST /api/plans/{id}/items/{item_id}/start`
- `POST /api/plans/{id}/items/{item_id}/complete`

CLI:
- `teamd-agent plans list <owner_type> <owner_id>`
- `teamd-agent plans show <plan_id>`
- `teamd-agent plans create <owner_type> <owner_id> <title>`
- `teamd-agent plans replace-items <plan_id> <json_or_file>`

- [ ] **Step 7: Run runtime/API/CLI plan tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/api ./internal/cli -run 'Plan' -count=1`
Expected: PASS

- [ ] **Step 8: Emit plan events and DTO propagation**

Add:
- `plan.created`
- `plan.updated`
- `plan.item_started`
- `plan.item_completed`

Ensure plan ids and summaries propagate through API DTOs.

- [ ] **Step 9: Add targeted tests for plan events and DTO propagation**

Assert:
- `plan.created`, `plan.updated`, `plan.item_started`, `plan.item_completed` are emitted
- plan API/CLI responses expose the expected ids, items, and notes

- [ ] **Step 10: Commit**

```bash
git add internal/runtime/types.go internal/runtime/store.go internal/runtime/plans_service.go internal/runtime/plans_service_test.go internal/runtime/sqlite_store.go internal/runtime/postgres_store.go internal/runtime/runtime_api.go internal/api/server.go internal/api/types.go internal/cli/client.go cmd/coordinator/cli.go
git commit -m "feat(teamD): add persistent plan state for runs and workers"
```

### Task 3: Worker Handoff Contract

**Files:**
- Create: `internal/runtime/handoffs_service.go`
- Create: `internal/runtime/handoffs_service_test.go`
- Modify: `internal/runtime/types.go`
- Modify: `internal/runtime/store.go`
- Modify: `internal/runtime/sqlite_store.go`
- Modify: `internal/runtime/postgres_store.go`
- Modify: `internal/runtime/workers_service.go`
- Modify: `internal/runtime/runtime_api.go`
- Modify: `internal/api/server.go`
- Modify: `internal/api/types.go`
- Modify: `internal/cli/client.go`
- Modify: `cmd/coordinator/cli.go`

- [ ] **Step 1: Write failing tests for worker handoff**

Cover:
- worker completion creates a structured handoff
- handoff references artifacts instead of large raw content
- parent can read handoff by worker id

- [ ] **Step 2: Run handoff tests to verify failure**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run TestWorkerHandoff -count=1`
Expected: FAIL because handoff types/service do not exist.

- [ ] **Step 3: Add handoff types**

Add:
- `WorkerHandoff`
- `PromotedFact`

`WorkerHandoff` must include:
- `summary`
- `artifacts`
- `promoted_facts`
- `open_questions`
- `recommended_next_step`

- [ ] **Step 4: Implement handoff persistence**

Add new storage:
- `runtime_worker_handoffs`

Persist one canonical handoff per finished worker execution cycle.

- [ ] **Step 5: Create handoff shaping logic**

`handoffs_service.go` should create a structured result from:
- worker summary text
- artifact refs
- explicit promoted facts
- open questions

- [ ] **Step 6: Integrate handoff into runtime and memory behavior**

Add runtime tests that prove:
- worker raw transcript is not promoted to shared memory by default
- only handoff/promoted facts are eligible for shared-memory promotion
- parent-facing result path prefers the handoff over raw worker transcript

- [ ] **Step 7: Expose handoff API and CLI**

Add:
- `GET /api/workers/{id}/handoff`

CLI:
- `teamd-agent workers handoff <worker_id>`

- [ ] **Step 8: Emit handoff events and DTO propagation**

Add:
- `worker.handoff_created`

Ensure handoff summary and artifact refs propagate through API DTOs where the parent needs them.

- [ ] **Step 9: Add targeted tests for handoff events and DTO propagation**

Assert:
- `worker.handoff_created` is emitted
- API/CLI responses expose `summary`, `artifacts`, `promoted_facts`, `open_questions`, `recommended_next_step`

- [ ] **Step 10: Run handoff tests**

Run: `GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/api ./internal/cli -run 'Handoff|Worker' -count=1`
Expected: PASS

- [ ] **Step 11: Commit**

```bash
git add internal/runtime/types.go internal/runtime/store.go internal/runtime/handoffs_service.go internal/runtime/handoffs_service_test.go internal/runtime/sqlite_store.go internal/runtime/postgres_store.go internal/runtime/workers_service.go internal/runtime/runtime_api.go internal/api/server.go internal/api/types.go internal/cli/client.go cmd/coordinator/cli.go
git commit -m "feat(teamD): add structured worker handoff contract"
```

### Task 4: Docs And Teaching Layer

**Files:**
- Create: `docs/agent/plans.md`
- Create: `docs/agent/artifact-offload.md`
- Modify: `docs/agent/04-tool-loop.md`
- Modify: `docs/agent/http-api.md`
- Modify: `docs/agent/cli.md`
- Modify: `docs/agent/workers.md`
- Modify: `docs/agent/06-compaction.md`
- Modify: `docs/agent/code-map.md`

- [ ] **Step 1: Write docs after code exists**

Explain:
- why artifact offload exists
- how plans differ from continuity/memory
- how worker handoff differs from worker transcript

- [ ] **Step 2: Add operator examples**

Include exact examples for:
- reading artifacts
- reading plans
- reading handoffs

- [ ] **Step 3: Verify docs links and referenced commands**

Run:
```bash
rg -n "artifact-offload|plans.md|handoff" docs/agent
```
Expected: references are consistent.

- [ ] **Step 4: Commit**

```bash
git add docs/agent/plans.md docs/agent/artifact-offload.md docs/agent/04-tool-loop.md docs/agent/http-api.md docs/agent/cli.md docs/agent/workers.md docs/agent/06-compaction.md docs/agent/code-map.md
git commit -m "docs(teamD): teach artifact offload plans and worker handoff"
```

### Task 5: Final Verification

**Files:**
- Modify: none necessarily

- [ ] **Step 1: Run targeted subsystem tests**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/api ./internal/cli ./internal/transport/telegram -count=1
```
Expected: PASS

- [ ] **Step 2: Run full suite**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./... -count=1
```
Expected: PASS

- [ ] **Step 3: Build coordinator**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go build ./cmd/coordinator
```
Expected: build succeeds with no output.

- [ ] **Step 4: Commit any final fixups**

```bash
git add -A
git commit -m "chore(teamD): finalize deep agents pattern rollout"
```

## Notes For Execution

- Keep diary updates in `memory/2026-04-12.md` or current day file after each modifying action, but do not include the diary in commits.
- Do not claim push. `origin` is still not configured in this repository.
- Prefer additive changes over broad refactors. This plan extends the current runtime; it does not replace it.
