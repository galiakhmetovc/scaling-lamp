# teamD Runtime Notes

Полная документация по текущему mesh runtime:
[docs/mesh-runtime.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/docs/mesh-runtime.md)

## Telegram Session Storage

- Telegram session history can run in-memory or in Postgres.
- When `TEAMD_POSTGRES_DSN` is set, the coordinator uses Postgres-backed session storage.
- The Postgres schema is auto-created at startup for MVP.
- Schema changes after the initial bootstrap are manual for MVP.
- Session limit currently counts individual messages, not user/assistant pairs.
- `/reset` clears the current chat session from the active store.

## Local Postgres Testing

- Postgres store tests use `TEAMD_TEST_POSTGRES_DSN` when set.
- If that variable is not set, tests default to `postgres://postgres:postgres@localhost:5432/postgres?sslmode=disable`.

## Go MCP Local Tools

- MCP local tools are implemented in-process in Go for MVP; there is no Node/npm gateway.
- Available tools currently include `filesystem.read_file`, `filesystem.write_file`, `filesystem.list_dir`, and `shell.exec`.
- The Telegram bot can now expose these tools to the model through `z.ai` function calling, so the model can decide to call them during a normal chat turn.
- Filesystem tool access is enforced under `TEAMD_MCP_FS_ROOT`; when unset in tests and local runtime, the default root is `/`.
- `shell.exec` currently has broad host access and is not sandboxed. The only built-in guards in this slice are timeout and interactive-flag rejection.
- Role-based allowlists, output caps, and stronger policy enforcement are tracked separately in `teamD-runtime-mcp-policy-baseline`.

## Mesh Runtime Summary

- Telegram ingress uses one user-facing `owner` agent.
- Owner can run clarification, proposal, winner-only execution, and composite planning flows.
- Peer agents communicate over HTTP `POST /mesh/message`.
- Registry and score storage are kept in Postgres.
- `/mesh` commands in Telegram control orchestration policy per session.
- The `Статус` button shows a detailed mesh trace split across multiple Telegram messages when needed.

See the full runtime description in:
[docs/mesh-runtime.md](/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/docs/mesh-runtime.md)
