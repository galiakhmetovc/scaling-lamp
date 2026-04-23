# Agent Schedule Edit UX Design

## Goal

Add full operator-facing create, edit, toggle, and delete controls for agent schedules in the canonical app/daemon/CLI/TUI path.

This slice extends the shipped schedule runtime without introducing a second management path or a schedule-only special case.

## Scope

This design covers:

- one canonical schedule update API in the app layer
- HTTP/daemon support for schedule updates
- CLI support for `create`, `edit`, `enable`, `disable`, and `delete`
- TUI schedule browser actions for create, edit, quick enable/disable, and delete
- a form-based TUI editor for both create and edit flows

This design does not cover:

- cron syntax
- trigger-based schedules
- batch schedule editing
- schedule history redesign
- broader session metadata polish outside the minimum refresh needed after schedule edits

## Constraints

- Preserve one canonical runtime path.
- Keep TUI and CLI thin over the same app/runtime layer.
- Reuse the existing `AgentSchedule` domain and persistence model.
- Do not add a second prompt path, second tool loop, or a schedule-specific execution path.
- Validate schedule semantics in one place so CLI, TUI, and daemon all behave the same.

## Current State

The current operator surface supports:

- list schedules
- show one schedule
- create a schedule
- delete a schedule

The operator surface does not yet support:

- editing an existing schedule
- toggling enabled state without manual record changes
- changing `mode`
- changing `delivery_mode`
- rebinding `target_session_id`
- editing the saved prompt

## Approaches

### Recommended: Canonical Update API + Form-Based TUI

Add one app-level update operation that accepts a validated schedule patch. Expose that patch through HTTP and CLI, and drive it from a single TUI form for create/edit plus a quick browser toggle.

Benefits:

- one source of truth for validation
- no drift between local app and daemon-backed clients
- TUI gets a usable operator workflow without inventing hidden state machines

### Alternative: CLI-Like Text Specs Inside TUI

Keep all schedule mutations as free-form command strings, even inside TUI dialogs.

This is faster to build but poor for discoverability and error recovery. It also wastes the existing browser/dialog structure already used by agents and artifacts.

### Alternative: Separate Endpoints For Every Mutation

Add dedicated app/HTTP methods like `enable_schedule`, `rebind_schedule`, `change_prompt`, `change_mode`, and so on.

This keeps each mutation small but bloats the operator surface and duplicates validation rules across nearly identical paths.

## Recommended Design

### Canonical App Layer

Introduce a schedule patch/update operation in `cmd/agentd/src/bootstrap/agent_ops.rs`.

The app layer owns:

- loading the existing schedule
- applying patch fields
- normalizing derived fields
- validating `AgentSchedule` invariants by rebuilding a canonical schedule value
- updating `updated_at`
- persisting the result

Create stays as a thin wrapper over the same normalized schedule-construction path.

### Patch Shape

The update payload should support optional changes to:

- `agent_identifier`
- `prompt`
- `mode`
- `delivery_mode`
- `target_session_id`
- `interval_seconds`
- `enabled`

`id` stays immutable for edit.

### Validation Rules

- `prompt` must remain non-empty
- `interval_seconds` must remain `> 0`
- `delivery_mode=fresh_session` clears `target_session_id`
- `delivery_mode=existing_session` requires non-empty `target_session_id`
- changing `agent_identifier` on an existing-session schedule must still point at a session whose `agent_profile_id` matches the schedule agent

Validation happens in the app layer and is enforced again by `AgentSchedule::new`.

### HTTP / Daemon Surface

Add one update route for schedules instead of multiple mutation-specific routes.

Recommended shape:

- `PATCH /v1/agent-schedules/{id}`

This keeps parity with existing session preference updates and lets daemon-backed CLI/TUI use the same patch semantics as local app mode.

### CLI Surface

Extend `/schedule` to support:

- `показать|show`
- `создать|create`
- `изменить|edit`
- `включить|enable`
- `выключить|disable`
- `удалить|delete`

Create and edit use explicit field syntax rather than positional overloads.

Recommended CLI spec:

- create:
  `id=<id> interval=<secs> mode=<interval|after_completion> delivery=<fresh_session|existing_session> [agent=<id>] [session=<session-id>] enabled=<true|false> :: <prompt>`
- edit:
  same format without `id=`, keyed by command target id

This avoids ambiguous positional parsing once mode and delivery are editable.

### TUI Surface

Use a form dialog, not a command-line mini language.

Add:

- `CreateScheduleForm`
- `EditScheduleForm`

Form fields:

- `id` for create only
- `agent`
- `mode`
- `delivery_mode`
- `target_session_id`
- `interval_seconds`
- `enabled`
- `prompt`

Schedule browser hotkeys:

- `Н` create
- `Р` edit
- `П` enable/disable quick toggle
- `У` delete
- `Enter` refresh preview

The preview pane continues to use `render_agent_schedule`, so there is still one render source.

## Data Flow

### Create

1. CLI or TUI collects operator input
2. thin client calls app or daemon update/create method
3. app resolves `agent_identifier`
4. app builds `AgentSchedule`
5. app persists via `AgentScheduleRecord`
6. browser/list preview refreshes from canonical render methods

### Edit

1. TUI loads the selected schedule into an edit form, or CLI parses the patch spec
2. client submits a schedule patch
3. app loads the current schedule
4. app applies patch fields and normalizes derived state
5. app validates by reconstructing a canonical `AgentSchedule`
6. app persists the updated record
7. CLI/TUI refreshes list and detail preview

### Quick Toggle

1. operator hits `П` in the schedule browser or runs `/schedule enable|disable`
2. client submits `{ enabled: true|false }`
3. app updates `enabled` and `updated_at`
4. browser row and preview refresh

## Error Handling

- Invalid CLI field names produce usage errors, not silent ignores
- Invalid TUI form submissions remain inside the dialog with a clear error
- Missing target session for `existing_session` remains a validation error
- If a target session no longer exists, that is allowed at edit time only if delivery is switched away from `existing_session`; otherwise the scheduler keeps its existing replacement behavior at runtime

## Testing

The implementation should follow TDD and cover:

- app-level create/edit/toggle semantics
- HTTP client/server update route
- CLI parsing for create/edit/enable/disable
- TUI browser hotkeys and form submission
- daemon-backed parity for schedule update flows

## Success Criteria

- An operator can create or edit every schedule field from TUI without touching raw records.
- CLI and daemon-backed CLI can perform the same schedule mutations through canonical commands.
- Enable/disable is a first-class action in both CLI and TUI.
- No second schedule-management path is introduced.
