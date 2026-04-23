# Agent Autonomy, Memory, and Competitive Research Design

## Goal

Define the next post-`1.0.0` runtime wave for:

- agent-controlled scheduling
- self-addressed deferred continuation
- controlled agent creation
- searchable session memory
- operator-approved rollout and self-update
- competitive catch-up versus Hermes, OpenFang, and adjacent projects

This design must preserve one canonical runtime path and avoid bolting on a second orchestration stack.

## Scope

This slice covers:

- research findings from the requested external systems and articles
- a gap matrix between teamD and competitor/self-described agent platforms
- concrete recommendations for new canonical tool surfaces
- a phased roadmap for autonomy, memory, and deployment work

This slice does not cover:

- automatic self-propagation across hosts
- worm-like or uncontrolled replication behavior
- implementation of the new features themselves

## Constraints

- Preserve one canonical runtime path.
- Keep TUI and CLI thin over the same app/runtime layer.
- Prefer structured bounded tools over prompt conventions or shell workarounds.
- Reuse the daemon, background jobs, schedules, agents, and offload layers that already exist.
- Any remote deployment or self-update must require explicit operator approval and explicit target selection.

## Current Baseline In teamD

Today teamD already has:

- one daemon-backed runtime path for chat, tools, background jobs, and schedules
- canonical structured filesystem, exec, planning, and offload tools
- built-in agent profiles with prompt files and tool allowlists
- inter-agent messaging and judge workflows
- recurring schedules managed by the daemon
- context offload and artifact-backed large output handling
- operator UX in CLI and TUI for sessions, agents, schedules, and inter-agent control

The main gaps are now higher-level autonomy and memory surfaces rather than core runtime plumbing.

## External Inputs

The following external materials were reviewed as direction signals:

- Claude Code token/context article:
  - https://thecode.media/claude-code-instrumenty-i-optimizaciya-tokenov/
- ruflo:
  - https://github.com/ruvnet/ruflo
- rufler:
  - https://github.com/lib4u/rufler
- ProjectEvolve:
  - https://github.com/Liflex/ProjectEvolve
- Hermes Agent:
  - https://github.com/nousresearch/hermes-agent
- OpenFang:
  - https://github.com/RightNow-AI/openfang
- claude-mem:
  - https://github.com/thedotmack/claude-mem
- memory-palace:
  - https://github.com/jeffpierce/memory-palace

These are treated as research inputs, not architectural mandates.

## External Findings

### Token and Context Economy

The Claude Code article reinforces a pattern already visible in teamD production behavior:

- input tokens dominate cost faster than output tokens
- large unfiltered context hurts reliability as much as cost
- retrieval must be selective and explicit, not broad by default

Implication for teamD:

- memory and search should retrieve narrow slices
- session history should be indexed and summarized deliberately
- tools that return large surfaces must stay bounded and cursor-based

### Hermes Agent

By its self-described repository surface, Hermes already leans into:

- persistent memory
- scheduling/cron
- MCP integration
- messaging gateways and broader operator surfaces
- approval/security controls

Implication for teamD:

- we are no longer behind on core runtime substrate
- we are still behind on exposed agent autonomy and memory breadth

### OpenFang

OpenFang presents itself as a broad agent-OS platform with:

- scheduler
- memory
- MCP
- A2A-style coordination
- wide tool breadth

Implication for teamD:

- treat OpenFang as a breadth benchmark
- do not copy breadth by adding sidecar stacks
- match it through one canonical runtime path with thinner, better-integrated surfaces

### ruflo and rufler

These are most useful as operator-experience references:

- orchestration over many roles/agents
- reviewable config-first lifecycle
- clear run/stop/preflight workflows

Implication for teamD:

- the best ideas to borrow are operator workflows and explicit configuration
- do not import a second orchestration runtime beside `agentd`

### ProjectEvolve

ProjectEvolve is valuable mainly as a reference for:

- persistent project knowledge across runs
- autonomous improvement loops over a durable project state

Implication for teamD:

- our session and project memory should become durable and searchable
- long-running autonomy should build on the existing background/schedule substrate

### claude-mem and memory-palace

These systems point at useful memory primitives:

- selective recall
- compressed summaries
- durable semantic memory
- graph-like relationships between memories

Implication for teamD:

- adopt native searchable memory and selective retrieval
- avoid opaque hook-magic as the primary memory model

## Gap Matrix

| Capability | teamD `1.0.0` | Hermes / OpenFang / others | Recommended Response |
| --- | --- | --- | --- |
| Canonical daemon runtime | Strong | Strong | Keep current path; do not fork |
| Agent-controlled scheduling | Weak | Stronger | Add canonical schedule tools for agents |
| One-shot self-resume | Missing | Often present implicitly | Add a delayed self-message / one-shot schedule path |
| Agent factory | Weak operator support only | Broader role spawning | Add controlled agent-create path with templates and policy |
| Searchable session memory | Weak | Stronger | Build retention + indexing + session search |
| Memory retrieval ergonomics | Partial via offload | Stronger | Add first-class session/memory retrieval tools |
| Remote rollout/self-update | Weak | Mixed | Add operator-approved deployment workflow |
| Uncontrolled self-propagation | Not present | Not a goal | Explicitly reject; use approved rollout only |
| Operator workflow polish | Medium | Often broader | Continue CLI/TUI productization on canonical app layer |

## Recommended Design Direction

### 1. Give Agents Canonical Schedule Tools

Agents need first-class schedule management through the same canonical tool surface used everywhere else.

Recommended new tools:

- `schedule_list`
- `schedule_create`
- `schedule_update`
- `schedule_delete`
- `schedule_read`

Key requirements:

- bounded outputs
- exact schedule ids
- no prompt-only conventions
- validation in one app/runtime layer
- same semantics for operator-created and agent-created schedules

### 2. Support Self-Addressed Deferred Continuation

The most useful immediate autonomy case is not full multi-agent spread. It is:

- an agent schedules a one-shot message to itself
- the daemon wakes that session later
- work resumes through the same canonical run path

Recommended semantics:

- either extend schedules with `once_at`
- or add a dedicated one-shot deferred message primitive backed by the same scheduler substrate

Preferred operator-visible behavior:

- “continue this later” becomes a first-class action
- the resumed work remains attributable to the original session and agent

### 3. Add a Controlled Agent Factory

Agents should be able to create other agents, but only through a constrained factory path.

Recommended scope:

- create from built-in template or existing agent
- set name
- set allowed tools or inherit a safe template allowlist
- optionally create an initial session for the new agent

Recommended constraints:

- exact policy validation in the app layer
- no arbitrary direct filesystem writes into agent homes by the model
- operator visibility for who created what

### 4. Build Searchable Session Memory

The next major gap is not “more context in prompt”. It is retrieval.

Recommended layers:

1. retention policy
   - active
   - warm archived
   - cold exported
2. indexing
   - sessions
   - transcript summaries
   - artifacts/offloads
   - explicit decisions/notes when available
3. retrieval tools
   - `session_search`
   - `session_read`
   - later, optional semantic recall on top of exact metadata search

This should stay native to teamD state instead of bolting on a separate memory daemon.

### 5. Replace “Self-Propagation” With Operator-Approved Rollout

Automatic self-propagation should not be a product goal.

Instead, the safe and useful version is:

- operator-approved remote deployment
- explicit approved target inventory
- explicit rollout plan
- auditable apply/update/revert actions

Recommended shape:

- a deployment plan is prepared by the agent or operator
- the operator approves explicit targets
- the daemon applies rollout through a controlled workflow

This gives the useful operational outcome without turning the system into uncontrolled self-spreading software.

## Proposed Import Order

### Phase 1: Research To Concrete Runtime Design

- `teamD-research.3` token optimization and memory patterns
- `teamD-research.2` orchestration references from ruflo, rufler, ProjectEvolve
- `teamD-research.1` final Hermes/OpenFang gap matrix

### Phase 2: Agent Autonomy

- `teamD-autonomy.1` canonical schedule-management tools
- `teamD-autonomy.3` self-addressed one-shot deferred continuation
- `teamD-autonomy.2` controlled agent factory

### Phase 3: Memory

- `teamD-memory.1` retention, archival, and retrieval policy
- `teamD-memory.2` indexing and search across sessions, transcripts, and artifacts

### Phase 4: Deployment

- `teamD-deploy.1` operator-approved remote deployment and self-update workflow

## Non-Goals And Red Lines

- No automatic self-propagation.
- No host-to-host spread without explicit operator approval.
- No second runtime, second prompt path, or special autonomous tool loop.
- No unbounded memory rehydration into the prompt.

## Decision

The next wave should optimize for:

- explicit agent autonomy through canonical schedule and agent-management tools
- durable searchable memory instead of wider implicit context
- operator-approved rollout instead of self-propagation

This is the shortest path to catching up with broader agent platforms without losing teamD's single-runtime architecture.
