# Runtime Comparison Report

This report summarizes how `teamD` compares with the external systems reviewed during the post-`1.0.0` research pass.

Detailed research notes live in:

- `docs/superpowers/specs/2026-04-22-external-runtime-capability-gap-matrix.md`

## Scope

Compared systems:

- `teamD`
- `OpenClaw`
- `Hermes Agent`
- `OpenFang`
- `ruflo`
- `rufler`
- `ProjectEvolve`
- `claude-mem`
- `memory-palace`

Primary goal:

- identify where `teamD` is already strong
- identify what the others do better
- extract concrete catch-up priorities without breaking the one canonical runtime path

## teamD Today

### Strong

- One canonical daemon-backed runtime path.
- Thin CLI, TUI, and HTTP layers over the same app/runtime substrate.
- Structured bounded tool surface for filesystem, web, exec, planning, offload, and agent/schedule management.
- Daemon-managed recurring schedules and background jobs.
- Inter-agent messaging and judge workflow.
- Bounded large-output handling through offload/artifacts.
- Live process observability through `exec_read_output`.
- Local self-update from workspace release build.

### Weak or Missing

- No first-class `session_search` and `session_read`.
- No durable semantic memory layer.
- No external memory-provider abstraction.
- No full MCP client/runtime exposure in the canonical operator path yet.
- No remote rollout inventory or remote self-update path yet.
- No channel-adapter/gateway surface comparable to OpenClaw, Hermes, or OpenFang.
- No first-class one-shot self-resume primitive yet.

## External Findings

### OpenClaw

Strong at:

- channel-first gateway UX
- session-oriented tools:
  - `sessions_list`
  - `sessions_history`
  - `sessions_send`
  - `sessions_spawn`
- cron with one-shot and recurring semantics
- multi-agent routing by channel/account/peer
- strong host-vs-sandbox operator model

Why it matters for `teamD`:

- best reference for session-history tooling
- best reference for channel and gateway product UX
- best reference for cron/job ergonomics before the Hermes memory layer

### Hermes Agent

Strong at:

- persistent memory
- `session_search`
- built-in cron with an agent-facing job tool
- cross-platform gateway delivery
- broad toolset and skill system
- subagent delegation and programmatic tool calling

Why it matters for `teamD`:

- clearest benchmark for productized memory and search
- ahead of us on scheduling ergonomics and operator-facing delivery surfaces

### OpenFang

Strong at:

- packaged autonomous workflows via Hands
- broad security surface
- MCP + A2A breadth
- channel adapters
- persistent memory and canonical sessions
- workflow engine breadth

Why it matters for `teamD`:

- breadth benchmark
- strongest reference for packaged autonomy and security hardening surface

### ruflo

Strong at:

- multi-agent orchestration breadth
- swarm coordination claims
- memory/routing/learning story

Most useful to borrow:

- orchestration vocabulary
- ideas for background worker visibility

What not to copy:

- a second orchestration engine beside `agentd`

### rufler

Strong at:

- preflight/doctor UX
- run registry
- task/run status
- token accounting
- resume semantics
- live follow/dashboard UX

Why it matters for `teamD`:

- best near-term operator UX reference

### ProjectEvolve

Strong at:

- durable project-improvement loop
- repeated analyze -> propose -> implement -> test -> document cycles
- accumulated project knowledge across iterations

Why it matters for `teamD`:

- good reference for long-horizon autonomous repo work

### claude-mem

Strong at:

- persistent session capture
- progressive disclosure retrieval
- token-cost-aware memory access

Why it matters for `teamD`:

- strongest reference for layered retrieval instead of eager context reinjection

### memory-palace

Strong at:

- semantic search
- knowledge-graph memory
- transcript reflection
- inter-instance messaging in the memory layer

Why it matters for `teamD`:

- strongest reference for phase-two semantic memory design

## Capability Matrix

| Capability | teamD now | Best external reference | Gap |
| --- | --- | --- | --- |
| Canonical single runtime path | Strong | teamD | None |
| Agent-managed recurring schedules | Present | OpenClaw / Hermes / OpenFang | Moderate |
| One-shot self-resume | Missing | OpenClaw / Hermes | High |
| Agent factory | Present but basic | Hermes / OpenFang | Moderate |
| Historical session search | Missing | OpenClaw / Hermes | High |
| Durable semantic memory | Weak | Hermes providers / memory-palace | High |
| Progressive disclosure retrieval | Partial | claude-mem | Moderate |
| Multi-agent operator UX | Good | rufler / OpenClaw / Hermes | Moderate |
| Workflow packaging | Weak | OpenFang | High |
| Channel delivery surface | Weak | OpenClaw / Hermes / OpenFang | High |
| Remote rollout / self-update | Weak | Hermes / OpenFang | High |
| Security hardening surface | Medium | OpenFang | Moderate |

## What teamD Should Do Next

### P0

- Add `session_search`.
- Add `session_read`.
- Define retention/archive policy for sessions, transcripts, and artifacts.
- Add a first-class one-shot self-resume path.
- Extend schedule semantics on top of the current scheduler substrate.

### P1

- Add remote rollout inventory and operator-approved rollout.
- Add release-tag-based remote self-update.
- Add a memory-provider abstraction for deeper semantic memory.
- Improve run/session analytics and follow/status UX.

### P2

- Add packaged autonomous workflows on top of current schedules/background jobs.
- Add optional channel adapters/gateway product surfaces.
- Add optional semantic graph-memory integrations.

## What Not To Do

- Do not add a second orchestration runtime beside `agentd`.
- Do not build hidden hook-magic memory as the primary architecture.
- Do not implement autonomous self-propagation.
- Do not chase breadth by breaking the canonical runtime path.

## Bottom Line

`teamD` is no longer behind on core runtime substrate. The real gap is now product surface:

- historical retrieval
- durable memory
- richer autonomy semantics
- rollout
- packaged workflows
- channel delivery

Best practical interpretation of the landscape:

- `OpenClaw` is the gateway/session/cron benchmark
- `Hermes` is the memory/search/scheduling benchmark
- `OpenFang` is the breadth/security/packaged-autonomy benchmark
- `rufler` is the operator-UX benchmark
- `claude-mem` and `memory-palace` are the retrieval/memory-architecture benchmarks
