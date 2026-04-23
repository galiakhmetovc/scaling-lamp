# External Runtime Capability Gap Matrix

## Goal

Compare teamD against the external systems requested by the operator and extract concrete catch-up priorities for post-`1.0.0` work.

This document is intentionally more concrete than the broader autonomy/memory design memo. It focuses on:

- what teamD demonstrably has today
- what the linked systems demonstrably expose
- what is worth importing into teamD
- what should stay out of teamD

## teamD Current Baseline

### Strong today

- One canonical daemon-backed runtime path, not split by UI surface.
  - `crates/agent-runtime/src/tool.rs`
  - `cmd/agentd/src/execution/provider_loop.rs`
- Thin CLI/TUI/HTTP layers over the same app/runtime substrate.
  - `cmd/agentd/src/cli/repl.rs`
  - `cmd/agentd/src/tui.rs`
  - `cmd/agentd/src/http/server.rs`
- Structured bounded tool surface for filesystem, web, exec, planning, offload, and agent/schedule management.
  - `crates/agent-runtime/src/tool.rs`
- Recurring background schedules already exist and are daemon-managed.
  - `cmd/agentd/src/execution/background.rs`
- Agent-facing schedule CRUD and agent creation now exist in the canonical tool path.
  - `cmd/agentd/src/execution/autonomy.rs`
- Inter-agent messaging, judge flow, and explicit continuation grants already exist.
  - `cmd/agentd/src/execution/interagent.rs`
  - `cmd/agentd/src/bootstrap/execution_ops.rs`
- Large-output handling is bounded through offload/artifact retrieval rather than prompt bloat.
  - `crates/agent-runtime/src/context.rs`
  - `cmd/agentd/src/execution/provider_loop.rs`
- Live process observability now exists through `exec_read_output`.
  - `crates/agent-runtime/src/tool.rs`
- Local self-update exists for replacing the currently running binary from the workspace release build.
  - `cmd/agentd/src/about.rs`

### Weak or missing today

- No first-class `session_search` / `session_read` tool for historical transcript recall.
- No durable semantic memory layer; current memory is small in-process note storage.
  - `crates/agent-runtime/src/memory.rs`
- No first-class external memory-provider abstraction.
- No native MCP client/runtime exposure in the canonical operator path yet.
- No remote deployment/rollout inventory or remote self-update path yet.
- No channel-adapter/gateway product surface comparable to Hermes/OpenFang.
- No explicit one-shot self-resume primitive yet; only recurring schedules and background substrate.

## External Sources Reviewed

- Claude Code token article:
  - https://thecode.media/claude-code-instrumenty-i-optimizaciya-tokenov/
- ruflo:
  - https://github.com/ruvnet/ruflo
- rufler:
  - https://github.com/lib4u/rufler
- ProjectEvolve:
  - https://github.com/Liflex/ProjectEvolve
- OpenClaw:
  - https://github.com/openclaw/openclaw
  - https://docs.openclaw.ai/
- Hermes Agent:
  - https://github.com/nousresearch/hermes-agent
  - https://hermes-agent.nousresearch.com/docs/
- OpenFang:
  - https://github.com/RightNow-AI/openfang
  - https://www.openfang.sh/
- claude-mem:
  - https://github.com/thedotmack/claude-mem
  - https://docs.claude-mem.ai/progressive-disclosure
- memory-palace:
  - https://github.com/jeffpierce/memory-palace

## Per-Project Findings

### Claude token article

Useful takeaways:

- input tokens dominate cost quickly in long-running sessions
- broad default context hurts reliability, not just cost
- root instructions should stay short and delegate detail to narrower references
- progressive disclosure beats eager retrieval

Import into teamD:

- keep root prompt assembly short and stable
- prefer index-first retrieval for memory/session search
- continue bounding large tool outputs instead of rehydrating them blindly

### ruflo

What it appears strong at:

- very broad multi-agent orchestration surface
- built-in swarm coordination and routing language
- memory/routing/learning story
- background services and heavy orchestration claims
- native Claude Code/Codex integration claims

What to borrow:

- ideas for explicit orchestration roles and routing vocabulary
- ideas for background supervision and long-running worker visibility

What not to copy:

- a second orchestration engine beside `agentd`
- a separate memory/router stack parallel to the canonical runtime path

Assessment:

- Good inspiration for breadth.
- Bad fit as a direct architectural import.

### rufler

What it is good at:

- reviewable single-file config
- strong preflight and doctor flow
- run registry across projects
- durable task/run status
- token accounting per run and per task
- resume semantics after interruption
- follow/dashboard UX for long-running multi-task work

Most useful imports for teamD:

- stronger run registry and per-run analytics
- better operator-facing follow/status dashboards
- richer token accounting in session/status views
- resume/restart semantics for higher-level background workflows

Assessment:

- Strong operator UX reference.
- Better source of ideas than ruflo for immediate productization.

### ProjectEvolve

What it is good at:

- durable project-improvement loop
- repeated analyze -> propose -> implement -> test -> document cycles
- accumulated project knowledge between iterations

Most useful import:

- durable project memory and iteration-aware autonomy over an existing repo

Assessment:

- Good reference for autonomous repo-improvement loops.
- Less relevant for runtime substrate, more relevant for long-horizon workflows.

### OpenClaw

What it demonstrably exposes:

- a local-first gateway with many messaging/channel integrations
- session-oriented tooling:
  - `sessions_list`
  - `sessions_history`
  - `sessions_send`
  - `sessions_spawn`
- cron jobs with one-shot and recurring semantics
- multi-agent routing by channel/account/peer
- companion-device and node surfaces
- host-vs-sandbox security model with explicit operator guidance
- browser/canvas/nodes/cron as first-class tool families

Why it matters:

- OpenClaw is a strong reference for channel-first operator productization.
- It is especially relevant for session history tooling, cron/job semantics, and “assistant across channels” UX.
- Hermes clearly builds on and extends some of this operator surface, so skipping OpenClaw would hide an important step in the landscape.

Where teamD is ahead:

- stricter canonical runtime-path discipline
- more explicit bounded-output handling via offload/artifacts
- cleaner daemon-first separation from UI surfaces

Where teamD is behind:

- session history/search as an agent-facing surface
- channel/gateway integrations
- packaged multi-surface operator flows
- richer remote/peripheral companion story

### Hermes Agent

What it demonstrably exposes:

- persistent memory and explicit memory/user profile split
- built-in `session_search` over stored sessions
- built-in cron scheduling with an agent-facing `cronjob(...)` tool
- cross-platform gateway surface
- broad toolset and skill system
- subagent delegation and programmatic tool calling
- explicit docs for security/approvals, MCP, memory, cron, and CLI/gateway behavior

Why it matters:

- Hermes is currently ahead of teamD on productized memory, search, scheduling ergonomics, and channel delivery.
- Its breadth is not just “more tools”; it has more finished operator workflows around those tools.

Where teamD is already competitive:

- canonical runtime discipline
- bounded tool surfaces
- daemon-first substrate
- TUI/CLI thinness over one runtime path

Where teamD is behind:

- searchable session history
- persistent memory beyond tiny note stores
- richer schedule semantics
- channel delivery/gateway story
- external memory-provider/product integrations

### OpenFang

What it demonstrably exposes:

- a very broad “agent OS” breadth claim
- autonomous “Hands” that run on schedules without prompting
- strong security/product packaging story
- large channel-adapter surface
- MCP + A2A + native protocol surfaces
- richer memory/session breadth
- workflow engine with triggers and step modes

Why it matters:

- OpenFang is a breadth benchmark.
- It is ahead of teamD on autonomous packaged workflows, channels, security hardening surface, and workflow-engine breadth.

Where teamD should respond:

- not by cloning its breadth wholesale
- but by matching the highest-value capabilities through the existing canonical runtime:
  - searchable sessions
  - durable memory
  - one-shot self-resume
  - operator-approved rollout
  - richer workflow packaging

### claude-mem

What it is good at:

- persistent session capture
- progressive disclosure for retrieval
- natural-language memory search
- explicit token-cost awareness
- citations and a web viewer

What to borrow:

- layered retrieval model:
  - index first
  - details second
  - source third
- explicit token-cost framing for retrieval tools

What not to copy:

- hook-heavy automatic context reinjection as the primary memory architecture

Reason:

- teamD should stay daemon-native and retrieval-explicit, not hook-magic-first.

### memory-palace

What it is good at:

- semantic search
- knowledge-graph memory
- transcript reflection into memory
- inter-instance messaging inside the memory layer
- code indexing via prose summaries instead of raw chunk embeddings

What to borrow:

- semantic memory as an optional deeper layer above exact session search
- transcript reflection as a memory-ingestion path
- graph-enriched recall for decisions/notes/code memories

What not to copy immediately:

- a full external graph-memory subsystem as the new core of teamD

Reason:

- teamD first needs exact searchable session history and retention policy.
- Semantic memory is phase two, not phase zero.

## Capability Matrix

| Capability | teamD now | Strongest external reference | Gap judgment | Recommended response |
| --- | --- | --- | --- | --- |
| Canonical single runtime path | Strong | Mixed | Ahead or equal | Preserve current design |
| Agent-managed recurring schedules | Present | OpenClaw / Hermes / OpenFang | Moderate gap | Keep current base; add one-shot and richer delivery semantics |
| One-shot self-resume | Missing | Hermes cron one-shots / OpenFang hands | Real gap | Add first-class delayed self-message or one-shot schedule |
| Agent factory | Present but basic | Hermes/OpenFang broader role packaging | Moderate gap | Expand controlled agent factory, templates, policy, visibility |
| Historical session search | Missing | OpenClaw / Hermes | High gap | Add `session_search` and `session_read` |
| Durable semantic memory | Weak | memory-palace / claude-mem / Hermes providers | High gap | Add retention/search first, semantic layer second |
| Progressive disclosure retrieval | Partial via offload | claude-mem | Moderate gap | Use index-first retrieval for session/memory surfaces |
| Multi-agent operator UX | Good | rufler / OpenClaw / Hermes | Moderate gap | Improve run registry, tokens, dashboards, follow views |
| Workflow packaging | Weak | OpenFang Hands / workflows | High gap | Add packaged autonomous workflows later, on top of schedules/background |
| Channel delivery surface | Weak | OpenClaw / Hermes / OpenFang | High gap | Defer until memory/search and rollout are done |
| Remote rollout / self-update | Weak | Hermes/OpenFang install/update breadth | High gap | Build operator-approved SSH/systemd rollout |
| Security hardening surface | Medium | OpenFang | Moderate gap | Import only targeted safeguards, not blanket framework breadth |

## What teamD Should Import Next

### P0

- `session_search`
- `session_read`
- retention/archive policy for sessions, transcripts, and artifacts
- one-shot self-resume to the same session/agent
- richer schedule semantics built on current scheduler substrate

### P1

- controlled remote rollout inventory
- release-tag-based remote rollout and self-update
- memory-provider abstraction for deeper semantic memory
- operator-visible run/session analytics inspired by rufler

### P2

- packaged autonomous workflows similar in spirit to Hands, but built on teamD schedules/background jobs rather than a second runtime
- optional channel adapters/gateway layer
- optional semantic graph memory integrations

## What teamD Should Explicitly Avoid

- self-propagating deployment behavior
- a second orchestration runtime beside `agentd`
- hook-driven hidden memory reinjection as the main architecture
- copying breadth without operator discipline or bounded tool contracts

## Bottom Line

The gap is no longer “teamD lacks a runtime.” The gap is now:

- searchable memory and historical retrieval
- more ergonomic autonomy surfaces
- packaged workflows
- deployment and operator convenience

Compared with the requested projects:

- OpenClaw is the clearest benchmark for channel-first gateway UX, session-history tools, and cron/job ergonomics.
- Hermes is the clearest benchmark for productized memory, search, scheduling, and gateway UX beyond OpenClaw.
- OpenFang is the clearest benchmark for breadth, packaged autonomy, and security surface.
- rufler is the best near-term operator UX reference.
- claude-mem and memory-palace are the best references for how to design retrieval without flooding context.

The correct response for teamD is not to become a second copy of any of them. It is to import the best ideas into the current canonical daemon/runtime path, in this order:

1. exact search and retention
2. one-shot self-resume
3. deeper memory
4. rollout
5. packaged workflows
