# Telegram Run Control UX Design

Date: 2026-04-10
Status: Approved in chat

## Goal

Add a Telegram-native control surface for live agent runs so the user can immediately see that a request was accepted, observe progress while the agent is working, cancel a run if needed, and request detailed execution statistics on demand.

This slice is explicitly about runtime control and observability in the Telegram chat. It is not a general session-management redesign.

## User Problem

The current bot behavior is technically functional but operationally awkward:

- after sending a request, the user cannot immediately tell whether the request reached the runtime
- long-running work is opaque unless raw tool output is streamed into the chat
- the user cannot cancel a run once it has started
- detailed token/tool statistics are always visible in the footer, even when the user does not need them
- Telegram formatting is brittle for tables and some markdown structures

The control UX must make the bot feel alive and controllable without spamming the chat.

## Scope

This design covers:

- request acknowledgement in Telegram
- one mutable status message per active run
- inline controls for `Status` and `Cancel`
- periodic status refresh during execution
- detailed stats on demand rather than in every final reply
- Telegram-safe output formatting, especially table reshaping

This design does not cover:

- approval workflows
- MCP policy enforcement
- subagent UI
- broad session-management UX beyond what is needed to support active runs

## UX Decisions

The following decisions are fixed for this slice.

### 1. Request Acknowledgement Is Mandatory

As soon as the user sends a request, the bot sends a short acknowledgement message such as:

`✅ Запрос получен, запускаю агент`

The acknowledgement also includes a simple waiting timer such as:

`Ждём ответ: 00:05`

This message exists even if the status card appears almost immediately. The purpose is operational confidence: if the pipeline later fails, the user still knows the request reached the runtime.

### 2. One Mutable Status Card Per Active Run

After the acknowledgement, the bot sends one status card message and edits that same message while the run is in progress.

The status card shows:

- current stage
- elapsed runtime
- a compact set of live badges such as current tool, provider state, and context-window percentage
- a live progress block labelled `Что уже сделано`

The status card must update automatically, with a target cadence of roughly every 5 seconds while the run is active.

The bot should prefer editing the existing message over sending new progress messages.

### 3. No Persistent Reply Keyboard

A persistent reply keyboard is out of scope for this flow because it wastes space in Telegram.

Control actions must appear as inline buttons attached to the active status card only.

### 4. Inline Controls

The active status card must expose inline buttons:

- `Статус`
- `Отменить`

`Статус` opens or updates a more detailed execution breakdown.

`Отменить` requests cancellation of the active run. It is only relevant while the run is active and should not remain visible on completed runs.

### 5. Progress Block Stays Expanded

The `Что уже сделано` block should not be collapsed for the first implementation.

It should remain visible in the status card and update as the run progresses. The goal is fast comprehension without extra taps.

### 6. Tool Activity Should Not Spam Separate Messages

Raw tool activity must stop appearing as separate Telegram messages during normal operation.

Instead:

- tool execution contributes to the live `Что уже сделано` block
- the status card reflects the current tool/stage
- detailed tool timing and counts appear behind the `Статус` action

Full raw tool output may still exist internally, but it should not be pushed into the chat as a stream of separate messages for this UX slice.

### 7. Detailed Statistics Are On-Demand

Detailed runtime statistics should not be appended to every final answer.

They are shown through the `Статус` action and may include:

- prompt tokens
- completion tokens
- tool-related token/input cost if available
- tool output size
- total tool execution time
- context-window usage percentage

The default final reply should stay readable and user-oriented.

### 8. Context Window Percentage Is Visible

The status UX should expose context-window usage as a percentage, since the runtime already has:

- the configured context window size
- local prompt-size estimates
- provider usage after successful calls

This percentage is useful live feedback during long runs.

### 9. Markdown And Table Formatting

Telegram output formatting should be adapted instead of passed through blindly.

The following formatting policy is fixed:

- keep headings, bullets, short code spans, and fenced code blocks where Telegram can render them safely
- transform unsupported or fragile structures into Telegram-safe plain text
- never send tables as literal tables to Telegram

Table policy:

- all tables are reshaped into plain-text list/card form
- do not use monospace table grids as the primary table strategy
- do not use image rendering for tables in this slice

Example:

Instead of:

`SERVICE | STATUS | REASON`

Send:

- `api`
  `Status: failed`
  `Reason: missing DB_URL`

## Runtime Model

For one Telegram request, the runtime flow becomes:

1. inbound user message received
2. acknowledgement message sent immediately
3. status card message sent
4. worker run starts
5. status card edited periodically while the run advances
6. `Что уже сделано` block updated from observed execution events
7. if user presses `Отменить`, runtime attempts cancellation and status card reflects that transition
8. if user presses `Статус`, bot returns or edits a detailed stats view
9. when the run finishes, the status card is finalized and the final answer is sent separately

## Data Needed For Status UX

The status view will need structured per-run data, at minimum:

- Telegram chat id
- run id
- status message id
- ack message id if later updates are desired
- current lifecycle stage
- started_at / elapsed time
- current tool name if any
- completed tool steps summary
- cancellation state
- token usage and local prompt estimate
- context-window percentage

This runtime state may be ephemeral for MVP, but it must be explicit rather than reconstructed from raw chat history.

## Failure Behavior

The UX must stay informative under failure.

- If the request is accepted but worker execution fails early, the acknowledgement remains as proof the request arrived.
- The status card should move to a failed state rather than silently disappear.
- If periodic refresh fails once, the run should continue and the next refresh attempt should retry.
- Cancellation should produce a visible terminal state such as `Отменено`.

## Testing Expectations

This slice should include:

- adapter tests for ack + status-card flow
- tests for edit-message updates instead of message spam
- callback tests for `Статус` and `Отменить`
- formatting tests for markdown adaptation and table reshape
- a live Telegram smoke check for one long-running request

## Non-Goals

- no rich graphical charts in Telegram
- no persistent bottom keyboard
- no raw per-tool message stream
- no image-based formatting fallback

## Follow-On Work

- richer status details presentation
- live expandable sections if Telegram UX proves it worthwhile
- policy-aware hiding of dangerous actions
- session-level dashboarding on top of the same run-state model
