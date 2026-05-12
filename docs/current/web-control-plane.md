# Web Control Plane

`apps/control-plane` is the imported React/Node web control-plane adapted from `hermes-workspace`.

## Status

The app is imported as a big-bang port. Many Hermes screens and endpoints are intentionally still present while they are adapted to `teamD`.

The first adapted boundary is:

- `GET /api/ping`
- `GET /api/teamd-status`

Both read `agentd`, not the Hermes gateway.

## Runtime Boundary

The control-plane is a web shell and adapter layer. It may own UI state, auth, rendering, and route normalization. It must not become a second agent runtime.

Canonical runtime responsibilities remain in `agentd`:

- session state;
- prompt assembly;
- model calls;
- tool execution;
- agent profiles;
- task registry;
- routing and delivery;
- Postgres persistence;
- NATS event flow.

## Configuration

Run the control-plane with:

```sh
cd apps/control-plane
pnpm install
TEAMD_AGENTD_BASE_URL=http://127.0.0.1:5140 pnpm dev
```

Optional:

```sh
TEAMD_AGENTD_TOKEN=...
```

## Imported Upstream

- Repository: `https://github.com/outsourc-e/hermes-workspace`
- License: MIT
- Local attribution: `apps/control-plane/LICENSE`
- Adaptation notes: `apps/control-plane/TEAMD_ADAPTATION.md`

## Adaptation Queue

The intended module order is:

1. Runtime health and snapshot.
2. Chat and sessions.
3. Agents and profile editing.
4. Tasks, routes, delivery targets, and schedules.
5. Files, artifacts, editor, and terminal.
6. Memory, KV, SilverBullet, skills, MCP, and tools.
7. Swarm/mesh operation views.
8. Settings, auth, deployment, and mobile/PWA polish.

