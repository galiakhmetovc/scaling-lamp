# TUI Hardening Design

## Goal

Make the TUI maintainable and operationally cleaner without changing the core runtime model.

This slice does two things together:

- decomposes the current monolithic TUI implementation into pane-focused modules
- polishes the existing pane UX so the result is not merely better structured, but also easier to operate

## Current Problems

The current TUI works, but `internal/runtime/tui/app.go` owns too much:

- app shell wiring
- pane state
- key handling
- pane rendering
- plan editing
- settings editing
- chat timeline rendering

That is already a maintainability risk. It also makes UX polish expensive because every change crosses unrelated responsibilities.

## Constraints

- keep `--chat` as the TUI entrypoint
- keep non-interactive fallback behavior for tests and scripted stdin
- keep runtime event bus + projections as the source of truth
- keep session-scoped plan semantics
- do not rewrite the TUI from scratch

## Recommended Structure

Split `internal/runtime/tui` into focused files:

- `app.go`
  - only top-level Bubble Tea model wiring
  - routes messages to panes
- `state.go`
  - shared TUI state structs
- `commands.go`
  - async commands such as chat turn execution and config rebuild
- `sessions_pane.go`
  - session list rendering and selection logic
- `chat_pane.go`
  - timeline rendering, live stream block, chat scrolling
- `plan_pane.go`
  - plan tree, selection, form-based editor, plan operator actions
- `tools_pane.go`
  - tool log list and details pane
- `settings_pane.go`
  - settings form/raw yaml rendering and interaction
- `render_markdown.go`
  - markdown rendering helpers

This keeps each pane as a focused unit with one clear purpose.

## Pane UX Hardening

### Chat

- keep markdown timeline
- keep live stream block
- add consistent scroll behavior
- keep input focus rules explicit
- separate chat history viewport from input handling

### Plan

- keep full plan tree only in `Plan`
- keep form-based editing
- make selection and edit state explicit
- improve operator feedback after edits
- preserve event-sourced writes through runtime operator methods

### Tools

- keep the live log
- add a selected-item detail pane
- make scrolling and selection explicit

### Settings

- keep `Session Overrides`, `Config Form`, `Raw YAML`
- give each mode stable scrolling/focus behavior
- stop mixing render/build state directly into generic app update paths

### Sessions

- keep the top-level session manager
- make mouse and keyboard selection logic live in one pane-specific place

## State Model

Use a shared top-level model plus pane-owned state blocks.

Shared app state should only contain:

- active tab
- active session id
- session registry / ordering
- top-level viewport sizing
- shared status and error banners
- pane state structs

Pane-specific cursor/focus/form fields should live with that pane instead of in a single flat app struct.

## Event Model

No domain rewrite is needed.

The TUI should continue to consume:

- persistent projections for durable state
- UI bus events for ephemeral stream/tool status

The hardening work is about cleaner composition, not about changing source-of-truth boundaries.

## Testing Strategy

Add focused pane-level tests instead of only one broad app smoke test.

Target coverage:

- chat pane timeline rendering and scrolling
- plan pane selection/editor actions
- tools pane selection and details
- settings pane mode transitions and save/apply behavior
- sessions pane selection and activation

## Out Of Scope

Not in this slice:

- full TUI rewrite
- drag-and-drop task editing
- manual tool execution UI
- schema-driven settings generation for every config module
