# teamD Clean-Room Rewrite

This branch is the root of the new agent rewrite.

Rules:

- legacy code is reference only
- new runtime is event-sourced
- behavior is expressed through policies, strategies, resolved contracts, and executors
- configuration is explicit, modular, and rooted at one config file per agent instance

Current baseline documents:

- `docs/superpowers/specs/2026-04-14-context-policy-design.md`
- `docs/superpowers/plans/2026-04-14-context-policy-implementation.md`
- `docs/superpowers/specs/2026-04-14-prompt-tools-policy-design.md`
- `docs/superpowers/plans/2026-04-14-prompt-tools-policy-implementation.md`

Operational baseline:

- build local binary:
  - `go build -o ./agent ./cmd/agent`
- smoke config:
  - `config/zai-smoke/agent.yaml`
- environment template:
  - `.env.example`
  - loaded automatically from `.env` by `cmd/agent` without overriding already-set env vars
- GitHub artifact workflow:
  - `.github/workflows/build-agent-artifact.yml`
  - uploads:
    - `teamd-agent-linux-amd64`
    - `teamd-agent-windows-amd64`

Run modes:

- smoke:
  - `./agent --config ./config/zai-smoke/agent.yaml --smoke "ping"`
- interactive chat:
  - `./agent --config ./config/zai-smoke/agent.yaml --chat`
- resume chat:
  - `./agent --config ./config/zai-smoke/agent.yaml --chat --resume <session-id>`

Chat mode baseline:

- real TTY:
  - full TUI workspace with tabs for sessions, chat, plan, tools, and settings
- non-interactive stdin:
  - fallback to legacy line-based chat loop
- transcript-backed resume via `TranscriptProjection`
- chat UX behavior still comes from `ChatContract` strategies and params

Current prompt and tool baseline:

- system prompt comes from file through `PromptAssemblyContract`
- session head is assembled from projections and placed at `messages[0]`
- visible tools are selected through `ToolContract`
- provider-emitted tool calls are gated through `ToolExecutionContract`
- current built-in allowed tool execution supports:
  - internal plan tools
  - workspace filesystem tools
  - bounded shell execution
- plan-management state is event-sourced and projected back into session head
- internal planning state is now session-scoped, so each chat session has its own active plan/head view

Current plan-tools docs:

- `docs/clean-room-plan-tools.md`
- `docs/superpowers/specs/2026-04-14-plan-tools-design.md`
- `docs/superpowers/plans/2026-04-14-plan-tools-implementation.md`

Current filesystem and shell docs:

- `docs/clean-room-filesystem-shell-tools.md`
- `docs/superpowers/specs/2026-04-14-filesystem-shell-tools-design.md`
- `docs/superpowers/plans/2026-04-14-filesystem-shell-tools-implementation.md`
