# Agent OS Design

## Goal

Introduce a global `AgentProfile` model so the operator selects an agent first and then creates sessions from that agent, while preserving one canonical runtime path for prompt assembly, chat execution, tool execution, approvals, and wake-ups.

The first built-in specialist agent is `judge`.

## Scope

This slice covers:

- global agent profiles with persisted current-agent selection
- immutable `session -> agent_profile_id` linkage
- agent-owned `SYSTEM.md`, `AGENTS.md`, and local `skills/`
- prompt assembly sourcing `SYSTEM.md` and `AGENTS.md` from the selected agent instead of the project workspace
- exact tool allowlists per agent
- built-in `default` and `judge` templates
- operator-visible agent management commands in TUI/REPL
- inter-agent messaging through the canonical session/runtime path
- loop protection for inter-agent chains with judge-mediated continuation
- compatibility with future scheduled agent launches through the existing daemon scheduler substrate

This slice does not cover:

- a second runtime path or special chat loop for agents
- project-owned prompt files participating in top-level prompt assembly
- remote A2A transport as the primary agent substrate
- per-agent model ownership
- mutable agent switching for already-created sessions
- full operator-facing schedule management UX

## Constraints

- Preserve one canonical runtime path. Agent sessions, judge sessions, operator sessions, and inter-agent wake-ups must all reuse the same chat/runtime/tool loop.
- Keep prompt assembly ordered as:
  1. `SYSTEM.md`
  2. `AGENTS.md`
  3. `SessionHead`
  4. `Plan`
  5. `ContextSummary`
  6. offload refs
  7. uncovered transcript tail
- `SYSTEM.md` and `AGENTS.md` for a session come from the agent home, not the project workspace.
- Session runtime settings such as `model` remain owned by the session and may diverge between sessions created from the same agent.
- Tools still operate in the project workspace, not in the agent home.
- Global skills remain available, but agent-local skills may override them by name.
- Tool permissions must be expressed as an exact allowlist of tool ids, not a role name or broad read/write flag.

## Approaches

### Recommended: Global Live Agent Profiles Over the Existing Session Runtime

Treat the agent as a live prompt/skills/tool-policy profile:

- the operator selects a current agent globally
- `\новая` creates a session bound to that agent
- each new turn in that session reads the agent's current `SYSTEM.md`, `AGENTS.md`, local skills, and allowed tool ids
- session-specific runtime settings such as `model` stay on the session

This matches the desired operator flow without inventing a second orchestration stack.

### Alternative: Session Snapshots Only

Copy all agent settings into the session at creation time and stop consulting the agent afterward.

This is simpler operationally, but it conflicts with the requirement that prompt files and skill overlays stay live per agent.

### Alternative: Fully Separate Runtime Per Agent

Treat each agent as an isolated runtime with its own execution stack, prompt pipeline, and orchestration semantics.

This looks closer to a full "agent OS", but it would violate the repository constraint against introducing a second runtime path and would create duplicated behavior in approvals, prompt assembly, tool policy, and wake-ups.

## Data Model

### AgentProfile

Add a persisted global `AgentProfile` with:

- `id`
- `name`
- `template_kind`
  - `default`
  - `judge`
  - `custom`
- `agent_home`
- `allowed_tools_json`
- `created_at`
- `updated_at`

`allowed_tools_json` stores the exact tool ids permitted for this agent.

### Global Runtime State

Persist one global `current_agent_profile_id` outside session state so the selected agent survives daemon restarts.

### Session Linkage

Sessions gain:

- `agent_profile_id`

This linkage is immutable after session creation.

Upgrade rule:

- pre-existing persisted sessions are backfilled to the built-in `default` agent during migration/bootstrap
- newly created sessions always persist the currently selected agent id at creation time

## Agent Home Layout

Each agent owns:

- `data_dir/agents/<agent-id>/SYSTEM.md`
- `data_dir/agents/<agent-id>/AGENTS.md`
- `data_dir/agents/<agent-id>/skills/`

Built-in templates `default` and `judge` exist from bootstrap time.

When the operator creates an agent from a template, the template contents are copied into a new independent `agent_home`.

If `SYSTEM.md` or `AGENTS.md` is missing from an agent home, runtime falls back to a built-in template for that missing file rather than failing the session.

## Prompt Assembly

The top-level prompt order stays unchanged, but the source of the first two files changes:

1. `agent_home/SYSTEM.md`
2. `agent_home/AGENTS.md`
3. `SessionHead`
4. `Plan`
5. `ContextSummary`
6. offload refs
7. uncovered transcript tail

Project workspace files do not contribute `SYSTEM.md` or `AGENTS.md` at this layer.

This preserves the canonical assembly contract while letting each agent own its identity and operating instructions.

## Skills

Effective session skills are built from:

1. the global skills catalog
2. `agent_home/skills`

If a skill name exists in both places, the agent-local skill wins.

This allows the `judge` template and future custom agents to replace a global skill with an agent-specific specialization without changing the global skill catalog.

## Tool Policy

Every session uses the canonical tool surface, but the effective visible surface is filtered through the bound agent's exact allowlist.

This must happen inside the canonical tool-routing path rather than by inventing separate agent-only tools or a special judge runtime.

For `judge`, the built-in allowlist excludes:

- file-writing tools
- `exec_start`
- `exec_wait`
- `exec_kill`

If the model requests a tool outside the allowlist, runtime should return a normal tool/policy denial through the existing tool error path rather than crashing the turn.

## Session and Operator UX

The operator flow is:

1. select an agent
2. create a new session
3. work inside that session with the bound agent profile

Minimum commands:

- `\агенты`
- `\агент показать`
- `\агент выбрать <id|name>`
- `\агент создать <имя> [из <template>]`
- `\агент открыть`
- `\новая`

CLI/TUI may keep slash aliases for compatibility, but the user-facing Russian command surface should document the backslash forms as canonical.

Startup behavior:

- if `current_agent_profile_id` exists, restore it
- otherwise bootstrap and select the built-in `default` agent

Minimum visibility:

- TUI header shows `агент=<name>`
- session list shows which agent a session belongs to
- `\система` and `\отладка` include:
  - `agent_profile_id`
  - `agent_home`
  - `system_path`
  - `agents_path`
  - `agent_skill_dirs`
  - `allowed_tools`

## Session Runtime Settings

The agent does not own the session `model`.

This means:

- two sessions created from the same agent may use different models
- an operator may change the model on an existing session at any time
- the live agent profile still supplies prompt files, skills, and tool policy for each new turn

This keeps agent identity separate from per-session runtime tuning.

## Inter-Agent Messaging

Agents must be able to send messages to each other through the same canonical chat/runtime path used for operator messages.

### Trigger Surface

For `v1`, inter-agent messaging is initiated by a canonical model-facing tool:

- `message_agent`

The tool takes the minimum structured input:

- `target_agent_id`
- `message`

It is not a text convention and not a special UI-only command.

On success, `message_agent` does not synchronously wait for the other agent's full answer inside the current turn. Instead it:

1. validates tool policy and inter-agent chain metadata
2. enqueues canonical daemon-owned follow-up work on the existing background/delegation substrate
3. returns an accepted/queued result to the current turn

Later, the origin session receives the other agent's answer through the same canonical wake-up path already used for background results.

This keeps inter-agent work on the existing background job, inbox event, and wake-up mechanisms rather than inventing a nested live chat loop.

### Base Semantics

- when one agent writes to another, runtime creates a new session for the recipient agent
- the recipient session processes the incoming message through the normal chat turn path
- the response is delivered back into the origin session through the same canonical message/wake-up path

For `v1`, this recipient session is one-shot per inter-agent send, not a reused long-lived thread:

- one outgoing inter-agent message creates one new recipient session
- that recipient session handles the request and produces one return payload to the origin session
- no automatic session reuse is attempted for later inter-agent sends, even if the same two agents talk again
- completed recipient sessions remain as normal inspectable sessions; they are not silently deleted

### Transcript and UI Semantics

The returned message should not pretend to be a human user message. It should be marked with the source agent, for example:

- `агент: judge`

This preserves traceability while keeping the delivery path identical to user-originated message handling.

### Chain Metadata

Inter-agent chains carry:

- `chain_id`
- `origin_session_id`
- `origin_agent_id`
- `hop_count`
- `parent_interagent_session_id`

This metadata is for routing, loop protection, and operator visibility.

## Loop Protection and Judge Continuation

Inter-agent messaging needs an explicit loop guard.

Recommended policy:

- default `max_hops = 3`
- `hop_count` increments when runtime accepts a `message_agent` call and creates the next recipient session
- when `hop_count >= max_hops`, further forwarding is denied by default
- `judge` may grant a one-time continuation for that specific chain

The continuation must be scoped to one chain, not globally disable loop protection.

This allows useful escalation chains while keeping accidental ping-pong between agents bounded and inspectable.

### Continuation Trigger and Persistence

For `v1`, continuation is granted by a second canonical tool id available only to agents whose allowlist includes it:

- `grant_agent_chain_continuation`

The minimum input is:

- `chain_id`
- `reason`

Runtime persists a durable one-shot grant keyed by `chain_id`. When a blocked chain later attempts one more `message_agent` hop:

- if an unused continuation grant exists, runtime consumes it and allows exactly one additional hop
- otherwise the hop remains blocked

The grant is single-use and must be marked consumed durably so restarts cannot accidentally permit more than one extension.

### Minimal Chain State Machine

The minimum chain lifecycle is:

1. `active`
2. `blocked_max_hops`
3. `continued_once`
4. terminal on completion or failure

This is enough to keep persistence, wake-up behavior, and tests deterministic without adding a second orchestration stack.

## Scheduled Agent Launches

The `agent os` design should stay compatible with future scheduled launches without making scheduling part of this initial slice.

The intended extension path is a new durable `AgentSchedule` model bound to:

- `agent_profile_id`
- `project_workspace`
- `schedule`
- `prompt`

On schedule fire:

- the daemon creates a fresh new session for that agent in that project workspace
- the saved prompt is injected as the session's incoming message
- execution then proceeds through the same canonical chat/runtime path

This should reuse the existing mission/scheduler, background job, inbox event, and wake-up substrate rather than introducing a separate scheduler-owned execution path.

## Built-In Templates

### default

The built-in general-purpose agent:

- own `SYSTEM.md`
- own `AGENTS.md`
- broad tool allowlist matching the current normal operator-facing runtime

### judge

The built-in review/supervision agent:

- own `SYSTEM.md`
- own `AGENTS.md`
- own `skills/`
- restricted exact tool allowlist without file writes or command execution

`judge` is a normal `AgentProfile`, not a special-case runtime mode.

## Error Handling

- Missing agent files use built-in fallbacks instead of aborting prompt assembly.
- Unknown agent selection returns a normal operator-visible error.
- Forbidden tools return policy denials through the standard tool error path.
- If inter-agent delivery cannot create the recipient session, the origin session receives a normal failure result.
- If a chain exceeds `max_hops` without judge continuation, runtime records a blocked result rather than silently dropping the message.

## Testing

Required coverage:

- bootstrap creates built-in `default` and `judge` profiles
- current agent selection persists across daemon restarts
- new sessions persist immutable `agent_profile_id`
- prompt assembly reads `SYSTEM.md` and `AGENTS.md` from `agent_home`
- missing prompt files fall back correctly
- effective skill catalog prefers agent-local skills over global skills
- tool filtering obeys exact `allowed_tools_json`
- judge sessions cannot reach file-write tools or `exec_*`
- TUI/REPL commands render correct agent metadata
- inter-agent send creates a new recipient session
- recipient replies are routed back to the origin session with agent-source labeling
- hop counting blocks loops at `max_hops`
- judge continuation grants exactly one scoped extension to a blocked chain

## Rollout Order

Recommended implementation order:

1. persistence and bootstrap for `AgentProfile`, `current_agent_profile_id`, and `session.agent_profile_id`
2. agent-home filesystem management and template copying
3. prompt assembly switch to agent-owned `SYSTEM.md` and `AGENTS.md`
4. effective skill merging and exact tool allowlist filtering
5. operator commands and TUI/REPL visibility
6. inter-agent messaging on top of the canonical session/runtime path
7. loop guard and judge-mediated continuation

## Follow-On Work

After this slice:

1. richer agent editing commands beyond template-copy creation
2. explicit artifact/result packaging patterns for inter-agent conversations
3. remote agent-to-agent routing on top of the same chain metadata and wake-up path
4. specialized additional built-in agent templates beyond `judge`
