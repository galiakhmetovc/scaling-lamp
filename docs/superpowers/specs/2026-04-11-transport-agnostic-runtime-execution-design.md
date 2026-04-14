# Transport-Agnostic Runtime Execution Design

## Goal

Move run start and approval-resume orchestration behind a runtime-owned execution service so that Telegram, HTTP API, CLI, and a future web UI rely on the same runtime execution entrypoints instead of transport-specific lifecycle code.

## Boundary

Runtime owns:

- run start orchestration
- approval continuation resume orchestration
- launch semantics for managed runs
- runtime-facing start request types

Transport owns:

- update normalization
- session UX
- status cards
- final reply delivery
- transport-specific immediate commands

## Core design

Introduce `internal/runtime/execution_service.go`.

It wraps `runtime.API` and accepts explicit transport hooks:

- `PrepareStart`
- `ExecuteStart`
- `PrepareApprovalResume`
- `ExecuteApprovalResume`

This keeps runtime in control of orchestration while letting each transport provide its own UI/session behavior without duplicating run lifecycle logic.

## Success criteria

- API server starts runs through runtime execution service
- Telegram `Reply` and `Dispatch` start runs through runtime execution service
- approval callback resume also goes through runtime execution service
- tests pass without relying on bootstrap-only wiring
