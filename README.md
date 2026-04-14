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

- multiline input
- send on double `Enter`
- `/help`, `/session`, `/exit`
- streaming text output
- transcript-backed resume via `TranscriptProjection`
- chat UX behavior comes from `ChatContract` strategies and params

Current prompt and tool baseline:

- system prompt comes from file through `PromptAssemblyContract`
- session head is assembled from projections and placed at `messages[0]`
- visible tools are selected through `ToolContract`
- provider-emitted tool calls are gated through `ToolExecutionContract`
- actual allowed tool execution is not implemented yet; allowed calls fail honestly after gating
