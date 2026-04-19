# Streaming Chat REPL Design

## Goal

Add live streaming to the existing `chat repl` operator surface without
introducing a second chat runtime, a UI-local state machine, or any parallel
source of truth.

## Scope

This slice only targets the terminal REPL in `agentd`.

It adds:

- streamed assistant text deltas
- one live-updating tool status line per tool step
- approval-driven updates on that same tool status line
- final persisted transcript and run state through the existing canonical path

It does not add:

- a TUI or web UI
- model thinking stream
- a second execution loop
- provider-specific UI logic outside the provider/execution boundary

## User Experience

The REPL keeps the current command surface:

- `/help`
- `/show`
- `/approve [approval-id]`
- `/exit`

Normal assistant text streams into the terminal as deltas instead of appearing
only at turn completion.

Tool work is rendered as a single evolving status line rather than a burst of
log lines. For a tool step such as `web_fetch`, the status line progresses
through:

- `requested`
- `waiting_approval`
- `approved`
- `running`
- `completed`
- `failed`

The final tool status line remains in the transcript view of the live session
output after completion.

## Architecture

The provider layer gains real streaming support where available.

- `z.ai chat/completions` uses SSE with `stream=true`
- `tool_stream=true` is enabled when tool definitions are present so tool call
  data can be assembled from streaming chunks
- provider streaming stays provider-scoped: it emits typed provider events, not
  REPL strings

The execution layer remains the owner of run lifecycle and tool transitions.

- it translates provider stream events into run mutations and execution events
- it is responsible for announcing tool status transitions
- approvals continue through the same persisted provider loop

The REPL remains a thin renderer.

- it consumes typed execution events
- it prints assistant text deltas
- it redraws exactly one active tool status line per tool step
- it never infers run state from ad hoc local heuristics

## Event Model

Streaming into the REPL should use a small typed event set:

- `AssistantTextDelta`
- `ToolStatusChanged`
- `ApprovalRequired`
- `ApprovalResumed`
- `TurnCompleted`
- `TurnFailed`

Provider-specific details such as SSE chunk layout stay inside the provider
driver implementation.

## Fallback Behavior

The first implementation slice may support true streaming for `z.ai` first and
leave other providers on the existing non-streaming fallback path. This is
acceptable as long as:

- the canonical chat runtime does not fork
- the fallback path still emits coherent final execution events
- provider capability checks remain explicit

## Testing

Required regression coverage:

- REPL prints assistant deltas as they stream
- REPL renders one tool status line that updates through approval and completion
- REPL leaves the final tool status line visible
- approval resume still works after REPL restart
- non-streaming providers continue to work through the same operator path
