# MCP Daemon Lifecycle Design

## Goal

Add daemon-managed lifecycle and configuration for `stdio` MCP connectors so the service runtime can persist connector definitions, supervise live connections, and expose connector status through the app/HTTP layer.

## Scope

This slice covers `teamD-mcp.1` only:

- persisted MCP connector configuration
- daemon-held runtime registry for live connector state
- `stdio` transport only
- supervision and restart by the daemon background worker
- app and HTTP lifecycle operations

This slice explicitly does **not** cover:

- MCP tools/resources/prompts in the canonical runtime path
- TUI/CLI operator screens and commands
- HTTP/SSE transports

## Dependency Choice

Use the official Rust SDK, `rmcp`, as the lower-level MCP client transport/protocol layer.

Rationale:

- it is the official Rust SDK for MCP
- it already supports `stdio` child-process clients via `TokioChildProcess`
- it gives us a real MCP client session without inventing a second protocol implementation

## Architecture

### Persisted Config

Connector configuration is stored in persistence as the source of truth. The file config may seed initial connectors, but the daemon store owns subsequent lifecycle mutations.

Each connector stores:

- `id`
- `transport = stdio`
- `command`
- `args_json`
- `env_json`
- `cwd`
- `enabled`
- `created_at`
- `updated_at`

### Live Runtime Registry

The daemon owns an in-memory shared registry of live MCP connectors. This registry is **not** a second runtime path; it is the daemon-side runtime state for the persisted connector set.

Each live entry tracks:

- state: `starting | running | stopped | failed`
- pid
- started_at
- stopped_at
- last_error
- restart_count

### Connector Worker Model

Each `stdio` connector runs in its own worker thread with an embedded Tokio runtime. The worker:

1. launches the configured child process
2. establishes an MCP client session through `rmcp`
3. marks the connector `running` once initialization succeeds
4. waits until the session exits or a stop/restart signal arrives

The shared registry owns control handles for stop/restart coordination and status snapshots for app/HTTP reads.

### Supervision

The existing daemon background worker becomes the supervisor for MCP connectors:

- enabled connectors are started when missing
- failed or stopped connectors are restarted on later ticks
- disabled connectors are stopped and kept down

This keeps MCP lifecycle inside the same daemon supervision path as the rest of the runtime.

## API Surface

### App Layer

Add lifecycle methods:

- `list_mcp_connectors`
- `get_mcp_connector`
- `create_mcp_connector`
- `update_mcp_connector`
- `restart_mcp_connector`
- `delete_mcp_connector`

### HTTP Layer

Add daemon JSON endpoints for the same lifecycle operations. They return connector config plus live runtime status when available.

## Persistence Rules

- connector config is durable
- runtime status is in-memory only
- deleting a connector removes its persisted config and stops its live worker
- disabled connectors stay persisted but are not running

## Invariants

- one connector id maps to at most one live worker
- disabled connectors never auto-restart
- `teamD-mcp.1` does not expose MCP capabilities to prompt/tool assembly
- connector lifecycle remains daemon-owned; TUI/CLI stay thin wrappers later
