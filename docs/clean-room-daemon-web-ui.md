# Clean-Room Daemon And Embedded Web UI

## Current Status

Implemented now:

- config-graph-driven `operator_surface` contract
- `--daemon` entrypoint in `cmd/agent`
- fail-closed daemon server validation for host/port/assets/transport paths
- HTTP endpoints:
  - `/healthz`
  - `<endpoint_path>/bootstrap`
  - `/config.js`
- WebSocket live subscription on configured `websocket_path`
- embedded React/Vite production web client served from Go `embed`
- `dev_proxy` asset mode support for future frontend development
- shared WebSocket command protocol for sessions/chat/plan/tools/settings
- TUI as a daemon client over the shared operator-surface transport

Not implemented yet:

- richer web client protocol coverage for queued-draft UX and settings conflict UX
- full revision-conflict UX in the browser for concurrent plan/settings edits

## Usage

With shipped `zai-smoke` config:

```bash
./teamd-agent --config ./config/zai-smoke/agent.yaml --daemon
```

The daemon bind and transport paths are read from the current config graph:

- host: `0.0.0.0`
- port: `8080`
- bootstrap: `/api/bootstrap`
- websocket: `/ws`

Open:

- `http://<host>:8080/`

For frontend development:

- run `npm install` once in `web/`
- run `npm run dev` in `web/`
- switch `operator_surface.web_assets.mode` to `dev_proxy`
- point `operator_surface.web_assets.dev_proxy_url` at the Vite dev server

If `operator_surface` bind config is missing or invalid, daemon startup fails closed.

## Goal

Move teamD from a process-local interactive UI model to a daemon-first architecture:

- one long-lived daemon owns runtime state
- `TUI` and `Web UI` become clients of the same daemon API
- the same sessions, event log, projections, plans, tools, approvals, and `/btw` state are visible from both clients
- the daemon architecture remains compatible with future mesh and remote delegation work

This replaces the current split where the TUI directly embeds the runtime in-process.

## Non-Goals

- no auth or multi-user access yet
- no hosted control plane yet
- no browser-only duplicate runtime state
- no separate frontend server in production
- no second event model just for web

## Core Architecture

### 1. Daemon Node

The daemon is the single source of truth for:

- `Agent`
- event log
- projections
- shell runtime
- delegate runtime
- chat sessions
- operator actions

The daemon exposes:

- HTTP endpoints for bootstrap, config snapshots, and embedded assets
- a WebSocket endpoint for live state and operator actions

### 2. Shared Client Protocol

Both `TUI` and `Web UI` consume the same daemon API:

- same session list
- same chat timeline
- same plan head and plan mutations
- same tools log, approvals, running commands
- same settings state
- same `/btw` side-run results

This avoids a split architecture where the TUI talks to runtime internals while the web UI talks to an API layer.

### 3. Shared State Model

The daemon remains runtime-first:

- persistent state:
  - event log
  - projections
- ephemeral state:
  - UI bus
  - active runs
  - streaming deltas
  - queued drafts
  - `/btw` side-runs

The web UI does not become a new source of truth.

## Config Graph Integration

Bind, assets, and web transport must be part of the current config graph, not ad hoc CLI flags.

Add a new contract family under the existing root config:

- `operator_surface`

That contract resolves policy families such as:

- `daemon_server`
- `web_assets`
- `client_transport`
- `settings`

### Daemon Server Policy

Required params:

- `listen_host`
- `listen_port`
- `enable_websocket`

Optional params:

- `public_base_url`
- `allow_origin_patterns`

Rules:

- startup is fail-closed if required bind params are missing
- `0.0.0.0` is allowed only when explicitly configured

### Web Assets Policy

Required params:

- `mode`

Supported modes:

- `embedded_assets`
- `dev_proxy`

Mode params:

- `embedded_assets`
  - serve compiled static assets from `embed.FS`
- `dev_proxy`
  - proxy to a Vite dev server during development

### Settings Policy

The operator surface now also resolves a revisioned settings policy.

Required params:

- `require_idle_for_apply`
- `form_fields`
- `raw_file_globs`

Form fields are schema-driven and define:

- user-facing `key`
- `type`
- target `file_path`
- target `yaml_path`
- optional `enum`

Rules:

- settings writes are revision-aware
- stale writes fail with a revision conflict
- apply is fail-closed when idle is required and daemon runtime still has active work
- disk writes roll back if agent rebuild fails

## CLI And Process Model

Target process model:

- `teamd-agent --daemon`
  - starts daemon runtime + HTTP + WebSocket + embedded assets
- `teamd-agent --chat`
  - runs TUI client against the daemon API
- browser
  - connects to the same daemon API

Current shipped behavior is daemon-first for both TUI and web.

## Concurrency Rules

If `TUI` and `Web UI` open the same session:

- both must receive live updates
- chat is multi-writer
- tool/plan/status updates are broadcast to both
- `/btw` runs are visible in both

Conflict policy:

- `Chat`
  - multi-writer allowed
  - append-only event order defines the visible history
- `Plan` and `Settings`
  - revision-aware writes
  - client must detect stale revision and reload or reconcile

## Web UI

Frontend stack:

- `React`
- `Vite`
- embedded static assets in Go via `embed`

Current web client behavior:

- bootstrap via `<endpoint_path>/bootstrap`
- live updates and commands through the configured WebSocket path
- tabs for `Sessions`, `Chat`, `Plan`, `Tools`, and `Settings`
- session-scoped chat timeline with queued drafts and `/btw`
- plan mutations through daemon plan commands
- approvals / deny / kill through shell commands
- revisioned settings form and raw YAML editing

Top-level tabs mirror the TUI:

- `Sessions`
- `Chat`
- `Plan`
- `Tools`
- `Settings`

### Chat

Must preserve current TUI semantics:

- markdown timeline
- tool and plan blocks rendered as markdown
- daemon `session` snapshot carries explicit `main_run` metadata so clients do not infer provider/model/timer state from transient websocket timing
- status bar with:
  - provider
  - model
  - wall-clock time
  - active main-run timer
  - approximate context tokens
  - queue length
  - active `/btw` count
- queued drafts with recall-back-into-input editing flow
- `/btw` side-runs rendered separately

### Plan

- read from `plan_head`
- form-based editing through daemon actions

### Tools

- pending approvals
- running shell commands
- tool log and details
- approve/deny/kill actions

### Settings

- session overrides
- config form
- raw YAML editor

## WebSocket Protocol

The daemon WebSocket supports:

- bootstrap/state snapshot
- incremental events
- operator commands

Command categories:

- chat send
- queue draft
- queue recall
- `/btw`
- plan mutations
- approve / deny / kill
- settings get / form.apply / raw.get / raw.apply
- session selection / creation

Event categories:

- session list updated
- chat timeline updated
- streaming delta
- run lifecycle
- tool lifecycle
- plan projection updated
- tools projection updated
- settings/config updated

## Runtime Refactor Requirements

### New daemon layer

Introduce a daemon/server package that wraps:

- `Agent`
- transport endpoints
- client subscriptions
- shared command dispatch

This layer should own client connection bookkeeping and fanout.

### TUI migration

TUI must stop treating `Agent` as its direct backend.

Instead:

- TUI becomes a daemon client
- TUI consumes daemon snapshots/events
- TUI sends commands over the same client protocol used by web

### Session state location

Session truth stays in runtime/daemon, not only inside each UI process.

That includes:

- active run metadata
- queue state
- `/btw` side-runs

## Mesh Compatibility

The daemon must be designed as a local node boundary.

Future mesh work should be able to add:

- remote node transport
- remote delegate backends
- node identity / trust
- remote event propagation

without rewriting the UI clients.

This is the main reason the TUI must eventually become a daemon client instead of remaining runtime-local.

## Delivery Phases

### Phase 1

- add operator surface contract/policies
- add daemon server skeleton
- add HTTP bootstrap + WebSocket transport
- add embedded asset serving

### Phase 2

- introduce daemon client abstraction
- move TUI onto daemon API
- preserve current TUI features through the new client boundary

### Phase 3

- add embedded React web app shell
- render Sessions / Chat / Plan / Tools / Settings
- connect websocket live updates

### Phase 4

- bring web UI feature parity with TUI
- support queue, status bar, `/btw`, plan forms, tools actions, settings apply

### Phase 5

- add revision-aware plan/settings conflict handling
- harden daemon protocol
- prepare for remote node / mesh integration
