# Testing

This page is the shortest useful map for testing the runtime. It is organized around the four subsystems that actually have focused test coverage:

- provider mocking
- approval flow
- artifact offload
- workers and jobs

## Mock Provider Usage

Use `provider.FakeProvider{}` when the test only needs a deterministic reply and does not care about model behavior.

Good fits:

- runtime boot and wiring tests
- worker lifecycle tests
- integration smoke tests that only need a provider-shaped response

Reference points:

- [`internal/provider/provider.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/provider.go)
- [`tests/integration/coordinator_flow_test.go`](/home/admin/AI-AGENT/data/projects/teamD/tests/integration/coordinator_flow_test.go)

`FakeProvider` echoes the last user message when one exists, otherwise it returns `"ok"`. That makes it good for stable assertions, but not for multi-step tool loops.

When a test needs multiple provider turns, tool calls, or explicit finish reasons, use a tiny scripted stub like the one in [`internal/runtime/conversation_engine_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine_test.go).

## Approval Flow Without Telegram

Test approvals at the service/API layer first. Telegram is just one transport for the same approval state machine.

Prefer this flow:

1. Create the approval service with `approvals.New(approvals.TestDeps())`.
2. Create a pending approval with `Create`.
3. Resolve it with `HandleCallback` using `ActionApprove` or `ActionReject`.
4. Assert the updated status and callback metadata.

Reference points:

- [`internal/approvals/service_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/approvals/service_test.go)
- [`internal/runtime/approval_store_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/approval_store_test.go)
- [`internal/runtime/runtime_api_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/runtime_api_test.go)

For transport glue, keep Telegram isolated with `httptest.NewServer` and the adapter tests in [`internal/transport/telegram/approvals_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/approvals_test.go). Those tests prove the callback path and resume path without talking to the real Telegram API.

If you need to verify approval continuation persistence, use the runtime store tests and assert:

- `requested_at`
- `decided_at`
- `decision_update_id`
- continuation cleanup after resume

## Artifact Offload Testing

Test artifact offload directly through `MaybeOffloadToolResult`.

The important split is:

- small output stays inline
- large output is persisted in the artifact store and the transcript gets a preview plus `artifact_ref`

Reference points:

- [`internal/runtime/artifact_offload_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/artifact_offload_test.go)
- [`internal/runtime/conversation_engine_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine_test.go)
- [`internal/artifacts/store.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/artifacts/store.go)

Use `artifacts.NewInMemoryStore()` in unit tests. Assert both sides of the behavior:

- the returned preview contains the artifact reference
- the full payload is retrievable from the store
- the offloaded flag is set only when thresholds are exceeded

When testing the tool loop, also assert that the shaped `tool` message stored in the transcript contains the short preview rather than the full raw payload.

## Worker And Job Testing

Treat workers and jobs as separate primitives.

Workers:

- use `NewWorkersService(...)` with stub store, transcript store, and run control
- test `Spawn`, `Message`, `Wait`, `Close`, and `Handoff`
- assert isolated worker chat/session IDs and handoff content

Jobs:

- use `NewJobsService(...)` with a stub store
- test `StartDetached`, `Logs`, `Cancel`, and persisted policy snapshots
- assert lifecycle events such as `job.created`, `job.started`, `job.completed`, and `job.cancelled`

Reference points:

- [`internal/runtime/workers_service_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/workers_service_test.go)
- [`internal/runtime/jobs_service_test.go`](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/jobs_service_test.go)
- [`tests/integration/coordinator_flow_test.go`](/home/admin/AI-AGENT/data/projects/teamD/tests/integration/coordinator_flow_test.go)

The integration test file is the best example of how these pieces fit together:

- `worker.TestDepsWithProvider(provider.FakeProvider{})` for provider-shaped worker smoke tests
- `worker.TestDepsWithCapabilities(...)` when you need skills or MCP context
- direct approval creation/handling for callback logic

## Minimal Command Checklist

Run these before you call the work done:

```bash
mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...
GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'Test(ArtifactOffload|JobsService|WorkersService|RuntimeAPI)'
GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run 'TestAdapter.*Approval'
GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run 'Test(Coordinator|TelegramApproval|Worker)'
```

If the full suite is too expensive for local iteration, keep the last three commands as the fast path and run the full suite before closing the task.
