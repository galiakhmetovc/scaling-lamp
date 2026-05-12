# teamD Control Plane Big-Bang Design

## Goal

Import `hermes-workspace` as a full React/Node control-plane for `teamD`, then adapt its screens and server adapters to `agentd`.

## Decision

The web UI will be a separate Node/React app. This is intentional. It may own browser UX, auth, UI routing, local UI cache, and adapter/proxy endpoints.

`agentd` remains the canonical runtime. The control-plane must not implement a second model loop, prompt assembler, tool executor, task registry, mesh router, or durable session store.

## Import Scope

Import the Hermes workspace source wholesale into a dedicated app directory. Keep the original MIT license notice and document attribution.

Initial retained surfaces:

- Chat and sessions.
- Dashboard and operations.
- Files/artifacts/editor.
- Terminal.
- Memory.
- Skills.
- MCP/tools.
- Jobs/schedules.
- Settings/providers.
- Swarm/agent IDE surfaces.
- Tasks.
- Mobile/PWA shell.

## Adaptation Boundary

The Node app talks to `agentd` through an adapter layer:

- `TEAMD_AGENTD_BASE_URL` points to the daemon HTTP API.
- Adapter routes map Hermes-style frontend calls to `agentd` endpoints.
- Missing `agentd` endpoints should be added to `agentd`, not worked around by direct Postgres writes.
- File edits for agent profiles should go through `agentd` APIs, not direct filesystem writes from Node.
- Node may proxy, normalize, paginate, and format responses for UI needs.

## First Working Slice

After import, the first slice should prove the boundary:

- The app builds independently.
- `/api/ping` returns control-plane health.
- A teamD-specific endpoint can read `agentd` status/snapshot.
- The UI can show runtime health using real `agentd` data.
- Legacy Hermes endpoints may still exist during transition, but should be marked as unadapted where they depend on Hermes-specific runtime behavior.

## Risks

- The imported app is large and has many Hermes-specific assumptions.
- A direct big-bang import may compile with unused legacy routes that are not yet wired to `agentd`.
- Electron/Claude-gateway paths are likely irrelevant for production teamD and should be pruned only after the import builds.

## Non-Goals For The First Commit

- Full behavioral parity for every screen.
- Production auth hardening.
- Replacing `agentd` `/web`.
- Replacing Telegram/TUI flows.

