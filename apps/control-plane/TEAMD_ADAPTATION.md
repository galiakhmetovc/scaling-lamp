# teamD Control Plane Adaptation

This app was imported from `hermes-workspace` and is being adapted as the teamD web control-plane.

## Upstream

- Source: `https://github.com/outsourc-e/hermes-workspace`
- License: MIT, preserved in `LICENSE`
- Import strategy: big-bang source import, then module-by-module adaptation

## Boundary

The React/Node control-plane is allowed to own:

- Browser UI.
- UI routing.
- Operator authentication/session cookie.
- UI-only cache and presentation state.
- Adapter/proxy endpoints.

The control-plane must not own:

- Model execution.
- Prompt assembly.
- Tool execution.
- Durable agent session state.
- Mesh routing decisions.
- Postgres writes for runtime-owned records.

Those stay in `agentd`.

## Runtime Configuration

Set the daemon URL with:

```sh
TEAMD_AGENTD_BASE_URL=http://127.0.0.1:5140
```

Optional bearer token forwarding is supported with:

```sh
TEAMD_AGENTD_TOKEN=...
```

## First teamD Endpoints

- `GET /api/ping` checks control-plane and `agentd` reachability.
- `GET /api/teamd-status` proxies `agentd` status and the existing web snapshot.

Hermes-specific endpoints are retained during the import, but they are considered unadapted until they are explicitly wired to `agentd`.

