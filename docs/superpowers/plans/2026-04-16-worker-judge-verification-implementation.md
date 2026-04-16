# Worker Judge Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a separate judge-daemon verification loop so the worker must emit a machine-readable completion report, the judge verifies real outcomes with tools, and failed verification sends remediation back to the worker instead of allowing false completion.

**Architecture:** Build this in four bounded layers: report emission, verdict model, remediation loop, and finalization gate. Keep the worker and judge as separate daemon roles with separate contracts/policies, and persist all verification state through events and projections so TUI/web can render it cleanly.

**Tech Stack:** Go runtime/daemon, existing contracts/policies/config graph, event log + projections, TUI/web daemon clients, React TypeScript surfaces for operator visibility.

---

### Task 1: Add completion report contract and runtime storage

**Files:**
- Create: `config/zai-smoke/contracts/completion-report.yaml`
- Create: `config/zai-smoke/policies/completion-report/schema.yaml`
- Create: `config/zai-smoke/policies/completion-report/storage.yaml`
- Modify: `config/zai-smoke/agent.yaml`
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/policies/registry.go`
- Create: `internal/runtime/projections/completion_report.go`
- Create: `internal/runtime/projections/completion_report_test.go`
- Modify: `internal/runtime/component_registry.go`
- Modify: `internal/runtime/eventing/events.go`

- [ ] **Step 1: Write the failing projection test**

Add `TestCompletionReportProjectionTracksLatestReportPerSession` in `internal/runtime/projections/completion_report_test.go`.

The test should:
- apply a synthetic `completion_report.recorded` event;
- verify the projection stores the report by `session_id`;
- verify later reports supersede earlier reports for the same session.

- [ ] **Step 2: Run the projection test and verify it fails**

Run:

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/projections -run TestCompletionReportProjectionTracksLatestReportPerSession -count=1
```

Expected: FAIL because the projection and event kind do not exist yet.

- [ ] **Step 3: Implement the contract and policy types**

Add contract and policy structs for:
- completion report schema
- completion report storage

Wire them through:
- `internal/contracts/contracts.go`
- `internal/runtime/contract_resolver.go`
- `internal/policies/registry.go`

- [ ] **Step 4: Implement the event kind and projection**

Add:
- `completion_report.recorded` event kind
- completion report projection
- registry wiring in `component_registry.go`

- [ ] **Step 5: Re-run the focused projection test**

Run the same `go test` command from Step 2.

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add config/zai-smoke/contracts/completion-report.yaml config/zai-smoke/policies/completion-report/schema.yaml config/zai-smoke/policies/completion-report/storage.yaml config/zai-smoke/agent.yaml internal/contracts/contracts.go internal/runtime/contract_resolver.go internal/policies/registry.go internal/runtime/projections/completion_report.go internal/runtime/projections/completion_report_test.go internal/runtime/component_registry.go internal/runtime/eventing/events.go
git commit -m "feat: add completion report contract and projection"
```

### Task 2: Require the worker to emit completion reports

**Files:**
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/tool_loop.go`
- Modify: `internal/runtime/session_operator.go`
- Create: `internal/runtime/completion_report.go`
- Create: `internal/runtime/completion_report_test.go`
- Modify: `config/zai-smoke/prompts/system.md`

- [ ] **Step 1: Write the failing runtime test**

Add `TestWorkerFinalizationEmitsCompletionReport` in `internal/runtime/completion_report_test.go`.

The test should:
- simulate a session with plan activity;
- trigger the worker finalization path;
- assert a completion report event is recorded;
- assert the report contains:
  - claimed outcomes
  - plan item mapping
  - verification steps

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime -run TestWorkerFinalizationEmitsCompletionReport -count=1
```

Expected: FAIL because no report emission exists yet.

- [ ] **Step 3: Implement minimal report emission**

Add a runtime helper that builds and records a completion report from:
- current plan state
- known artifact refs
- recent verification actions

Do not add judge logic yet.

- [ ] **Step 4: Update the worker prompt contract**

Adjust `config/zai-smoke/prompts/system.md` so the worker is instructed to supply machine-checkable verification steps and not claim completion without them.

- [ ] **Step 5: Re-run the test**

Run the same `go test` command from Step 2.

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime/chat.go internal/runtime/tool_loop.go internal/runtime/session_operator.go internal/runtime/completion_report.go internal/runtime/completion_report_test.go config/zai-smoke/prompts/system.md
git commit -m "feat: require worker completion reports"
```

### Task 3: Add judge contract, verdict events, and daemon exposure

**Files:**
- Create: `config/zai-smoke/contracts/judge.yaml`
- Create: `config/zai-smoke/policies/judge/model.yaml`
- Create: `config/zai-smoke/policies/judge/prompt.yaml`
- Create: `config/zai-smoke/policies/judge/tools.yaml`
- Create: `config/zai-smoke/policies/judge/verdict.yaml`
- Create: `config/zai-smoke/policies/judge/orchestration.yaml`
- Modify: `config/zai-smoke/agent.yaml`
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/policies/registry.go`
- Modify: `internal/runtime/eventing/events.go`
- Create: `internal/runtime/projections/judge_verdict.go`
- Create: `internal/runtime/projections/judge_verdict_test.go`
- Modify: `internal/runtime/daemon/session_snapshot.go`
- Modify: `internal/runtime/daemon/server_test.go`
- Modify: `web/src/lib/types.ts`

- [ ] **Step 1: Write the failing verdict projection test**

Add `TestJudgeVerdictProjectionTracksLatestVerdictPerSession`.

- [ ] **Step 2: Run it and verify it fails**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/projections -run TestJudgeVerdictProjectionTracksLatestVerdictPerSession -count=1
```

- [ ] **Step 3: Implement the contract and verdict projection**

Add:
- judge contract families
- verdict event kinds
- verdict projection

- [ ] **Step 4: Surface verdict state through daemon snapshot**

Extend daemon session/bootstrap payloads to include:
- latest completion report summary
- latest judge verdict
- verification gate state

- [ ] **Step 5: Re-run focused tests**

Run:

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/projections ./internal/runtime/daemon -run 'Test(JudgeVerdictProjectionTracksLatestVerdictPerSession|SessionSnapshotIncludesJudgeState)' -count=1
```

- [ ] **Step 6: Commit**

```bash
git add config/zai-smoke/contracts/judge.yaml config/zai-smoke/policies/judge/model.yaml config/zai-smoke/policies/judge/prompt.yaml config/zai-smoke/policies/judge/tools.yaml config/zai-smoke/policies/judge/verdict.yaml config/zai-smoke/policies/judge/orchestration.yaml config/zai-smoke/agent.yaml internal/contracts/contracts.go internal/runtime/contract_resolver.go internal/policies/registry.go internal/runtime/eventing/events.go internal/runtime/projections/judge_verdict.go internal/runtime/projections/judge_verdict_test.go internal/runtime/daemon/session_snapshot.go internal/runtime/daemon/server_test.go web/src/lib/types.ts
git commit -m "feat: add judge verdict contract and daemon exposure"
```

### Task 4: Add local judge daemon runtime and manual verification trigger

**Files:**
- Create: `internal/runtime/judge_runtime.go`
- Create: `internal/runtime/judge_runtime_test.go`
- Modify: `internal/runtime/daemon/commands.go`
- Modify: `internal/runtime/daemon/protocol.go`
- Modify: `internal/runtime/daemon/server_test.go`
- Modify: `web/src/App.tsx`
- Modify: `web/src/tools/ToolsPane.tsx`
- Modify: `internal/runtime/tui/*` as needed for manual trigger affordance

- [ ] **Step 1: Write the failing daemon command test**

Add a test proving a manual `judge.run` command:
- accepts a session id;
- starts a judge run;
- records a verdict.

- [ ] **Step 2: Run the test and verify it fails**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime/daemon -run TestWebsocketJudgeRunCommandProducesVerdict -count=1
```

- [ ] **Step 3: Implement minimal local judge runtime**

The first slice should:
- load judge contracts;
- execute a bounded judge run locally;
- record verdict events;
- avoid remediation loop for now.

- [ ] **Step 4: Add manual trigger to daemon and operator surfaces**

Expose a `judge.run` command in:
- daemon websocket command protocol;
- TUI;
- web operator UI.

- [ ] **Step 5: Re-run the focused test**

Use the same command from Step 2.

- [ ] **Step 6: Commit**

```bash
git add internal/runtime/judge_runtime.go internal/runtime/judge_runtime_test.go internal/runtime/daemon/commands.go internal/runtime/daemon/protocol.go internal/runtime/daemon/server_test.go web/src/App.tsx web/src/tools/ToolsPane.tsx internal/runtime/tui
git commit -m "feat: add local judge runtime and manual verification"
```

### Task 5: Add remediation loop from judge back to worker

**Files:**
- Modify: `internal/runtime/judge_runtime.go`
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/daemon/commands.go`
- Create: `internal/runtime/judge_remediation_test.go`
- Modify: `internal/runtime/projections/judge_verdict.go`

- [ ] **Step 1: Write the failing remediation test**

Add `TestJudgeFailVerdictQueuesWorkerRemediation`.

The test should prove:
- a `fail` verdict emits structured remediation;
- worker receives it as actionable continuation input;
- the session does not finalize.

- [ ] **Step 2: Run the test and verify it fails**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime -run TestJudgeFailVerdictQueuesWorkerRemediation -count=1
```

- [ ] **Step 3: Implement remediation events and worker continuation**

Keep the first version simple:
- remediation becomes a structured operator/worker message event;
- worker continues in the same session with explicit findings.

- [ ] **Step 4: Re-run the test**

Use the same command from Step 2.

- [ ] **Step 5: Commit**

```bash
git add internal/runtime/judge_runtime.go internal/runtime/chat.go internal/runtime/daemon/commands.go internal/runtime/judge_remediation_test.go internal/runtime/projections/judge_verdict.go
git commit -m "feat: add judge remediation loop"
```

### Task 6: Add finalization gate

**Files:**
- Create: `config/zai-smoke/contracts/finalization-gate.yaml`
- Create: `config/zai-smoke/policies/finalization-gate/policy.yaml`
- Modify: `config/zai-smoke/agent.yaml`
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/runtime/contract_resolver.go`
- Modify: `internal/runtime/chat.go`
- Create: `internal/runtime/finalization_gate_test.go`

- [ ] **Step 1: Write the failing gate test**

Add `TestFinalizationRequiresJudgePassWhenPolicyEnabled`.

- [ ] **Step 2: Run the test and verify it fails**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/runtime -run TestFinalizationRequiresJudgePassWhenPolicyEnabled -count=1
```

- [ ] **Step 3: Implement the finalization gate**

Use the verdict projection and policy to decide whether finalization is allowed.

- [ ] **Step 4: Re-run the test**

Use the same command from Step 2.

- [ ] **Step 5: Commit**

```bash
git add config/zai-smoke/contracts/finalization-gate.yaml config/zai-smoke/policies/finalization-gate/policy.yaml config/zai-smoke/agent.yaml internal/contracts/contracts.go internal/runtime/contract_resolver.go internal/runtime/chat.go internal/runtime/finalization_gate_test.go
git commit -m "feat: gate finalization on judge verdict"
```

### Task 7: Documentation and operator workflow

**Files:**
- Modify: `docs/clean-room-daemon-web-ui.md`
- Modify: `docs/clean-room-tui.md`
- Modify: `README.md`

- [ ] **Step 1: Document the worker/judge model**

Explain:
- worker daemon role
- judge daemon role
- completion report contract
- remediation loop
- finalization gate

- [ ] **Step 2: Document operator actions**

Show:
- how to trigger verification manually;
- how to inspect verdicts;
- what blocked finalization means.

- [ ] **Step 3: Commit**

```bash
git add docs/clean-room-daemon-web-ui.md docs/clean-room-tui.md README.md
git commit -m "docs: document worker judge verification workflow"
```

### Task 8: Full verification

**Files:**
- Verify only

- [ ] **Step 1: Run focused package tests**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/config ./internal/runtime ./internal/runtime/projections ./internal/runtime/daemon -count=1
```

- [ ] **Step 2: Run broader regression**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go test ./internal/... ./cmd/agent -count=1
```

- [ ] **Step 3: Build**

```bash
TMPDIR=$PWD/.tmp-goexec/tmp GOCACHE=$PWD/.tmp-goexec/gocache go build ./cmd/agent
```

- [ ] **Step 4: If web dependencies are installed, run web verification**

```bash
cd web
./node_modules/.bin/vitest --run
npm run build
```

- [ ] **Step 5: Commit any remaining test/docs adjustments**

```bash
git add .
git commit -m "test: verify worker judge verification loop"
```
