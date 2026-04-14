# Full Operator TUI Design

Date: 2026-04-12
Status: draft

## Goal

Build a full terminal operator IDE for `teamD` on top of the existing runtime control plane.

The new canonical terminal interface is:

```bash
teamd-agent tui
```

The TUI must let an operator:

- talk to the agent in a persistent session
- watch live runs, workers, jobs, approvals, plans, memory, and artifacts
- inspect replay/debug information without leaving the terminal
- make approval decisions from a menu, not by typing opaque ids
- edit project-scoped policy in a formal YAML source-of-truth
- apply policy changes live

## Non-Goals

- no separate runtime path beside the existing API/control plane
- no hidden chain-of-thought display
- no global host-wide permanent policy store
- no Web UI in this slice
- no mesh hot-path adoption in this slice

## Canonical Runtime Boundary

The TUI is not a runtime feature. It is a client over existing runtime surfaces:

- `AgentCore`
- HTTP API
- SSE event stream
- approvals
- jobs
- workers
- plans
- artifacts
- replay
- session/control actions

This is a hard rule:

- no new terminal-only orchestration logic
- no second approval model
- no terminal-only session state that competes with runtime state

## Product Shape

The TUI is both:

1. operator console
2. debug console

It replaces the current line-based `chat` as the primary interactive terminal UX.

The existing `teamd-agent chat` remains only as a fallback path for:

- pipes
- non-interactive use
- minimal environments

## Layout

The initial full-screen layout is multi-pane.

### Left Pane

Purpose:

- session navigation
- plan visibility
- quick context switching

Contents:

- recent sessions
- active session marker
- recent plans for current session
- compact plan status summary

### Center Pane

Purpose:

- primary chat transcript
- agent conversation and operator interaction

Render blocks:

- `you`
- `assistant`
- `tool`
- `approval`
- `worker`
- `job`
- `plan`
- `memory`
- `system`

### Right Pane

Purpose:

- live control-plane visibility
- incident/debug surface

Contents:

- live events feed
- pending approvals
- active workers
- active jobs
- current run/control status
- memory/compaction/artifact signals

### Bottom Input Bar

Purpose:

- rich line editing
- command input
- shortcuts/help

Required:

- history
- cursor navigation
- completion
- explicit mode/status hints

## Modes

The TUI has three top-level modes.

### Normal Mode

Default operator mode:

- chat with the agent
- approve/reject actions
- inspect sessions/workers/jobs/artifacts

### Debug Mode

Focused operational mode:

- rawer event visibility
- state transition emphasis
- approval/policy provenance
- active run/worker/job metadata

### Replay Mode

Inspection mode for completed runs:

- ordered replay steps
- event correlation
- final response
- artifact refs
- related plans and handoffs

## Session UX

The operator should not need to memorize `chat_id` and `session_id` for normal use.

Required entry paths:

- `teamd-agent tui`
  - opens last active operator session
- `teamd-agent tui new`
  - creates/selects a new session
- `teamd-agent tui list`
  - lists sessions
- `teamd-agent tui use <session>`
  - switches active session

Internally, the runtime still uses:

- stable `chat_id`
- named `session_id`

But the TUI owns the human-friendly session switching UX.

## Approval UX

Approvals are not treated as binary yes/no prompts.

Each approval opens an action menu with these operator actions:

- `deny once`
- `deny and reply`
- `allow once`
- `allow for session`
- `allow forever`
- `allow all for session`

### Semantics

`deny once`
- reject only this approval

`deny and reply`
- reject and attach operator-visible response text

`allow once`
- approve only this approval

`allow for session`
- create a session-scoped rule for the current session

`allow forever`
- create a project-scoped persistent rule in policy YAML

`allow all for session`
- create a broad session-scoped rule for the matching policy class

## Policy Model

The source-of-truth for durable operator policy changes is:

- project-scoped YAML
- with comments

This YAML is not transport-specific.

It is a formal policy document that the runtime can parse and apply.

### Rule Scope

Rules must support more than a bare tool name.

A rule may match on:

- tool name
- action class
- optional argument or pattern scope
- session scope when temporary

The policy model must be strict enough that “allow weather lookup” does not accidentally mean “allow all `shell.exec` forever”.

## Policy Editor

The TUI includes a formal policy editor/view.

Required capabilities:

- browse effective rules
- inspect rule source
- create new rules from approval flow
- edit existing rules
- disable rules
- delete rules

Live behavior:

- save applies immediately
- runtime picks up updated effective policy without explicit reload command

## Event Model In TUI

The TUI consumes the existing persisted event plane and SSE stream.

Required visible categories:

- run lifecycle
- approval lifecycle
- worker lifecycle and handoff
- job lifecycle
- plan changes
- memory/compaction
- artifact offload
- replay references

Readable summaries are preferred over raw JSON, but debug mode may expose richer detail.

## Artifact And Handoff Inspection

The TUI must let the operator inspect:

- artifact metadata
- artifact content preview
- worker handoff summaries
- promoted facts
- open questions

This should not require leaving the TUI or dropping to a second command.

## Replay Integration

Replay mode is driven by the existing replay surface.

The TUI should:

- list recent runs
- open replay for a selected run
- correlate replay steps with visible artifacts/events when available

This is for debugging and operator inspection, not for re-execution.

## Terminal UX Requirements

The TUI requires a proper terminal line editor and key-driven navigation.

Minimum expected behavior:

- arrow-key navigation
- scrolling panes
- command/history recall
- completion
- modal menus
- status line

This is the main reason the existing line-based `chat` is no longer sufficient.

## Architecture Constraints

The TUI may add:

- terminal UI package(s)
- local UI state
- layout/render helpers
- client-side session UX helpers

The TUI must not add:

- runtime-only terminal logic
- alternate approval persistence
- duplicate policy semantics
- terminal-only event model

## Rollout Plan

### Phase 1

- TUI shell
- multi-pane layout
- session list
- center chat
- live event feed
- input editor/history/completion

### Phase 2

- approval action menu
- worker/job/plan side panels
- artifact/handoff inspection
- chat-first operator flow

### Phase 3

- formal project policy YAML model
- live policy apply
- TUI policy editor

### Phase 4

- replay/debug mode
- richer state inspection
- possible downgrade of old `chat` to compatibility mode only

## Risks

### Scope Risk

This is a large interface product, not a small CLI tweak.

Mitigation:

- phase delivery
- keep runtime boundary strict

### Plumbing Risk

Terminal UI work can consume time without improving runtime capability.

Mitigation:

- every TUI feature must sit on existing runtime surfaces
- no fake/demo-only state

### Policy Risk

Approval convenience can accidentally widen permissions too much.

Mitigation:

- formal YAML source-of-truth
- project scope only
- visible rule inspection and editing

## Recommendation

Proceed with the full TUI, but ship it in phases with a strict rule:

- every phase must already be useful to an operator
- no phase may invent a second runtime model
