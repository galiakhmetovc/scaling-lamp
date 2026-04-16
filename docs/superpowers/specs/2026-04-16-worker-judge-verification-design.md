# Worker Judge Verification Design

## Goal

Add a two-daemon verification architecture where the main worker daemon performs the task, emits a machine-readable completion report, and a separate judge daemon verifies the claimed work with real tools. If verification fails, the judge sends structured remediation back to the worker so execution continues instead of ending with a false "done".

## Why

The current runtime can work longer and preserve context better, but it still trusts the worker too much at finalization time. The log evidence already shows two classes of risk:

- the model can use tools in a way that looks plausible but is operationally wrong;
- the model can claim task completion without an independent check that artifacts, files, commands, and plan state really match the claim.

The missing piece is not another prompt tweak. It is a separate verification loop with an independent model, independent policy surface, and direct tool-based access to the same workspace facts.

## Non-Goals

- The judge is not a permanent second model observing every token.
- The judge does not silently rewrite the worker's answer.
- The judge does not share mutable in-process runtime state with the worker.
- The first slice does not require full mesh support; local daemon-to-daemon orchestration is enough.

## High-Level Model

There are two long-lived daemon roles:

- `worker daemon`
  - owns the main execution loop;
  - performs filesystem, shell, plan, artifact, and delegation work under the normal operator contracts;
  - must emit a `completion_report` before it can request finalization.

- `judge daemon`
  - runs on a separate model and separate prompt/contracts;
  - receives the worker's completion report plus factual runtime state;
  - uses verification tools to confirm or reject the worker's claims;
  - returns a structured verdict;
  - on failure, emits remediation instructions that are fed back to the worker.

This is a two-contour execution system:

1. worker executes;
2. worker emits `completion_report`;
3. judge verifies;
4. if `fail`, worker continues with remediation;
5. if `pass`, finalization is allowed.

## Required Worker Output

The worker must produce a machine-readable `completion_report`. A plain natural-language "done" is insufficient.

Minimum schema:

- `session_id`
- `run_id`
- `summary`
- `claimed_outcomes[]`
- `plan_items[]`
  - `task_id`
  - `claimed_status`
  - `evidence_refs[]`
- `artifacts[]`
  - `artifact_ref`
  - `purpose`
- `filesystem_outputs[]`
  - `path`
  - `expected_state`
- `verification_steps[]`
  - `kind`
  - `tool`
  - `payload`
  - `expected_result`

The completion report is part of the runtime protocol, not a UI-only decoration.

## Judge Inputs

The judge must be able to inspect:

- active plan
- archived plans when relevant
- transcript and summaries when needed
- completion report
- artifact store references
- filesystem state
- shell verification commands and their outputs
- current settings/policies that affect validation

The judge should prefer the completion report as the worker's explicit claim surface, then validate those claims against actual runtime facts.

## Judge Outputs

The judge returns a structured verdict:

- `status`
  - `pass`
  - `fail`
  - `inconclusive`
- `summary`
- `findings[]`
- `failed_checks[]`
- `missing_artifacts[]`
- `plan_gaps[]`
- `remediation_steps[]`

`pass` means finalization is allowed.

`fail` means the worker must continue.

`inconclusive` means the judge could not prove correctness or incorrectness. Policy decides whether this blocks finalization.

## Runtime Flow

### 1. Worker execution

The worker runs normally and maintains plan, artifacts, transcript, summaries, and verification evidence.

### 2. Completion report emission

When the worker believes it is done, it emits a `completion_report` event and persists the report in a runtime-visible store/projection.

### 3. Judge invocation

The orchestration layer starts a judge run using:

- completion report
- session snapshot
- plan snapshot
- artifact references
- relevant transcript/summary context

### 4. Tool-based verification

The judge executes the verification steps or refines them with allowed verification tools. It must not trust the worker's claimed command output without re-running or re-reading as required by policy.

### 5. Verdict handling

- `pass`: mark verification passed and allow finalization.
- `fail`: persist verdict and emit remediation message to worker.
- `inconclusive`: persist verdict and follow configured gate policy.

### 6. Remediation loop

When the verdict is `fail`, the worker receives structured remediation input and continues the same session instead of being considered complete.

## Contracts And Policies

This needs a dedicated contract family, not ad hoc flags spread across unrelated contracts.

### `JudgeContract`

Policy families:

- `judge_model`
  - separate model selection for judge daemon
- `judge_prompt`
  - verification-specific prompt assets and behavioral rules
- `judge_tool_access`
  - which tools the judge may use
- `judge_verdict`
  - verdict semantics, blocking rules, retry rules
- `judge_orchestration`
  - when to run judge
  - max verification rounds
  - remediation loop limits

### `CompletionReportContract`

Policy families:

- `report_schema`
  - required sections and fields
- `report_requirements`
  - which categories must be present for different task classes
- `report_storage`
  - where reports are persisted and exposed

### `FinalizationGateContract`

Policy families:

- `require_judge_pass`
- `allow_inconclusive`
- `max_remediation_cycles`

## Isolation Requirements

The judge daemon must be isolated from the worker in the ways that matter:

- separate prompt contract
- separate model policy
- separate process
- separate verification run ids
- no shared in-process mutable reasoning state

The judge may still read the same factual workspace and artifact surfaces, because that is the point of verification.

## Operator Experience

The operator surface should show:

- latest completion report
- latest judge verdict
- verification status badge
- remediation cycle count
- explicit reason why finalization is blocked

The operator should also be able to:

- manually trigger verification
- inspect judge findings
- stop the loop
- override the gate only if policy allows it

## Event And Projection Model

New event families:

- `completion_report.recorded`
- `judge.run.started`
- `judge.check.completed`
- `judge.verdict.recorded`
- `judge.remediation.issued`
- `judge.finalization.blocked`
- `judge.finalization.passed`

New projections:

- `completion_report`
- `judge_verdict`
- `judge_status`

## Safety Rules

- The worker cannot self-certify completion with plain text.
- The judge cannot silently mutate worker output.
- The judge should not execute broad mutation tools; verification should default to read/inspect/re-run validation surfaces.
- The remediation loop must be bounded by policy.

## Recommended Delivery Phases

### Phase 1

- completion report schema and storage
- worker finalization requires report emission

### Phase 2

- local judge daemon
- structured verdict model
- manual verification trigger

### Phase 3

- remediation loop from judge to worker
- finalization gate on `judge pass`

### Phase 4

- richer operator UX
- policy-driven automatic verification on finalization or milestones

## Acceptance Criteria

- A worker session cannot finalize without a completion report when the report contract is enabled.
- A judge daemon can verify a worker session using tools and emit a structured verdict.
- A `fail` verdict can feed structured remediation back to the worker.
- Finalization is blocked when policy requires `judge pass` and the verdict is not `pass`.
- Operator surfaces can inspect the report, verdict, and gate state without reading raw event logs.
