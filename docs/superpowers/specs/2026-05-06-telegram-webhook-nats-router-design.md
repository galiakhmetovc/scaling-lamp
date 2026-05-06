# Telegram Webhook NATS Router Design

Status: approved baseline for implementation planning.

## Decisions

- Telegram moves from long polling to webhook.
- The webhook endpoint lives inside the existing `agentd` HTTP server.
- NATS JetStream is a mandatory dependency for the event-driven runtime.
- There is no long-polling fallback in this architecture.
- Router decisions are deterministic and rule-based. No LLM router in the first implementation.
- Router configuration is stored in PostgreSQL because it must later be editable from a web UI.
- PostgreSQL remains the source of truth for durable state. JetStream transports events and provides backpressure/replay.
- The session/runtime core remains canonical. Webhook, NATS, router and workers must not create a second chat path or a second prompt path.
- Agent-agent and subagent work are one delegation plane. Subagent is bounded delegation with stricter context/tool/write-scope policy.
- Delivery targets are routing/fan-out, not an agent interaction mechanism.

## Goal

Move external Telegram input into a durable event-driven path:

```text
Telegram -> Caddy -> agentd webhook ingress -> NATS JetStream -> router -> session queue -> agent worker -> output queue -> delivery worker -> Telegram
```

The purpose is to make `teamD` a real MIMO runtime: one session can receive from many inputs, route to different agents, and publish outputs to one or more delivery targets without tying the session state to a single Telegram chat.

## Non-Goals

- Do not implement an LLM-based router in this phase.
- Do not replace PostgreSQL with NATS as the source of truth.
- Do not introduce a special Telegram model loop.
- Do not route Telegram updates directly into provider execution from the webhook handler.
- Do not build the web UI in this phase.
- Do not add an unbounded synchronous agent-agent RPC path.

## Interaction Model

The architecture has two planes:

1. `Delegation`: agent -> agent and agent -> subagent.
2. `Routing/Delivery`: session output -> delivery targets.

`Subagent` is not a separate transport. It is delegation with bounded context, explicit write scope, limited tools, timeout and structured return format.

`Delivery Target` is not agent communication. It is fan-out of already-created session outputs.

Human-facing input remains conversational. Agent-facing input must be structured.

## High-Level Flow

```text
Telegram
  -> Caddy HTTPS reverse proxy
  -> POST /v1/telegram/webhook/{secret}
  -> Telegram webhook ingress
  -> inbound_events row
  -> JetStream subject teamd.input.telegram
  -> Router worker
  -> routed_events row
  -> JetStream subject teamd.session.<session_id>.input
  -> Agent worker
  -> canonical App/runtime chat execution
  -> transcripts/artifacts/runs rows
  -> JetStream subject teamd.session.<session_id>.output
  -> Delivery worker
  -> session_output_routes + delivery_targets
  -> Telegram sendMessage/sendDocument
```

## Component Boundaries

### Telegram Webhook Ingress

Responsibility:

- Accept Telegram webhook requests through the current `agentd` HTTP server.
- Validate webhook secret.
- Parse Telegram update JSON.
- Compute a deterministic dedupe key.
- Persist the inbound event to PostgreSQL.
- Publish a pointer event to JetStream.
- Return `200 OK` quickly.

It must not:

- Execute a chat turn.
- Call the LLM provider.
- Send Telegram replies except operational webhook errors if needed.
- Make routing decisions beyond source metadata extraction.

### Event Store

PostgreSQL stores durable event state:

- raw or normalized inbound event metadata;
- routing decisions;
- delivery status;
- dedupe keys;
- worker processing state if needed for diagnostics.

NATS stores event streams for transport, replay and consumer coordination.

### Router Worker

Responsibility:

- Consume `teamd.input.*` events.
- Load router rules from PostgreSQL.
- Resolve target `session_id`, `agent_id`, priority and queue policy.
- Persist routed event.
- Publish `teamd.session.<session_id>.input`.

Router must be deterministic. Given the same event and same rules, it must produce the same routing decision.

### Agent Worker

Responsibility:

- Consume `teamd.session.*.input`.
- Load session and agent profile.
- Run the canonical app/runtime chat path.
- Persist runs, transcripts, tool calls and artifacts.
- Publish output events.

The agent worker must not know Telegram chat ids. Telegram is a delivery target, not core state.

### Delivery Worker

Responsibility:

- Consume output events.
- Load `session_output_routes` and `delivery_targets`.
- Apply format policy and send policy.
- Deliver to Telegram or future targets.
- Persist delivery status and route cursor.

Delivery failure must not roll back the completed run. It creates delivery status/errors instead.

## PostgreSQL Tables

### `event_sources`

Represents an external input source.

Suggested fields:

```text
source_id TEXT PRIMARY KEY
kind TEXT NOT NULL
address TEXT NOT NULL
display_name TEXT
owner_user_id TEXT
auth_policy_json TEXT NOT NULL DEFAULT '{}'
default_route_policy_json TEXT NOT NULL DEFAULT '{}'
enabled BOOLEAN NOT NULL DEFAULT TRUE
created_at BIGINT NOT NULL
updated_at BIGINT NOT NULL
```

For Telegram:

- `kind = telegram_private | telegram_group | telegram_topic | telegram_channel`
- `address = chat_id` or `chat_id:thread_id`

### `router_rules`

Rule-based routing configuration.

Suggested fields:

```text
rule_id TEXT PRIMARY KEY
priority BIGINT NOT NULL
enabled BOOLEAN NOT NULL DEFAULT TRUE
source_filter_json TEXT NOT NULL DEFAULT '{}'
operator_filter_json TEXT NOT NULL DEFAULT '{}'
condition_json TEXT NOT NULL DEFAULT '{}'
route_policy_json TEXT NOT NULL
created_at BIGINT NOT NULL
updated_at BIGINT NOT NULL
```

`route_policy_json` can express:

- `agent_id`
- `session_strategy`
- `session_id` for explicit route
- `queue_policy`
- `priority`
- `output_targets`
- `format_policy`
- `tool_policy`
- `retry_policy`
- `quiet_hours`
- `labels`

### `inbound_events`

Durable record of normalized incoming external events.

Suggested fields:

```text
event_id TEXT PRIMARY KEY
dedupe_key TEXT NOT NULL UNIQUE
source_kind TEXT NOT NULL
source_id TEXT NOT NULL
operator_id TEXT
payload_json TEXT NOT NULL
metadata_json TEXT NOT NULL DEFAULT '{}'
status TEXT NOT NULL
received_at BIGINT NOT NULL
published_at BIGINT
error TEXT
```

### `task_registry`

Unified async work registry.

Suggested fields:

```text
task_id TEXT PRIMARY KEY
kind TEXT NOT NULL
source_session_id TEXT
owner_agent_id TEXT
executor_agent_id TEXT
parent_task_id TEXT
status TEXT NOT NULL
dependency_json TEXT NOT NULL DEFAULT '[]'
context_ref_json TEXT NOT NULL DEFAULT '[]'
result_ref_json TEXT
retry_policy_json TEXT NOT NULL DEFAULT '{}'
attempt_count BIGINT NOT NULL DEFAULT 0
max_attempts BIGINT NOT NULL DEFAULT 1
timeout_at BIGINT
chain_id TEXT
hop_count BIGINT
max_hops BIGINT
trace_id TEXT
created_at BIGINT NOT NULL
updated_at BIGINT NOT NULL
started_at BIGINT
finished_at BIGINT
error TEXT
```

Task kinds:

- `agent_task`
- `delegate`
- `schedule_fire`
- `delivery`
- `tool_background`
- `webhook`

Task status:

- `queued`
- `running`
- `waiting_dependency`
- `waiting_input`
- `completed`
- `failed`
- `timed_out`
- `cancelled`
- `dead_lettered`

### `routed_events`

Durable record of router decisions.

Suggested fields:

```text
routed_event_id TEXT PRIMARY KEY
inbound_event_id TEXT NOT NULL REFERENCES inbound_events(event_id) ON DELETE CASCADE
rule_id TEXT
session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE
agent_id TEXT NOT NULL
queue_policy TEXT NOT NULL
priority BIGINT NOT NULL
payload_json TEXT NOT NULL
metadata_json TEXT NOT NULL DEFAULT '{}'
status TEXT NOT NULL
routed_at BIGINT NOT NULL
published_at BIGINT
error TEXT
```

### `event_outbox`

Optional but recommended bridge from PostgreSQL to NATS when an operation must be atomic with database writes.

Suggested fields:

```text
outbox_id TEXT PRIMARY KEY
subject TEXT NOT NULL
payload_json TEXT NOT NULL
status TEXT NOT NULL
attempt_count BIGINT NOT NULL DEFAULT 0
next_attempt_at BIGINT NOT NULL
created_at BIGINT NOT NULL
published_at BIGINT
last_error TEXT
```

### `event_deliveries`

Tracks delivery of output events to external targets.

Suggested fields:

```text
delivery_event_id TEXT PRIMARY KEY
source_event_id TEXT NOT NULL
target_id TEXT NOT NULL
status TEXT NOT NULL
attempt_count BIGINT NOT NULL DEFAULT 0
created_at BIGINT NOT NULL
updated_at BIGINT NOT NULL
delivered_at BIGINT
last_error TEXT
```

## NATS JetStream

Required streams:

```text
TEAMD_INPUT
  subjects:
    teamd.input.*

TEAMD_SESSION
  subjects:
    teamd.session.*.input
    teamd.session.*.output

TEAMD_DELIVERY
  subjects:
    teamd.delivery.*

TEAMD_TASKS
  subjects:
    teamd.task.*

TEAMD_DLQ
  subjects:
    teamd.dlq.*
```

Recommended message envelope:

```json
{
  "event_id": "evt_...",
  "event_type": "telegram.update.received",
  "trace_id": "trace_...",
  "source_kind": "telegram_private",
  "source_id": "telegram:123",
  "subject": "teamd.input.telegram",
  "payload_ref": {
    "kind": "postgres",
    "table": "inbound_events",
    "id": "evt_..."
  },
  "created_at": 1770000000
}
```

NATS payloads should be small pointers or bounded normalized payloads. Large raw updates, files and artifacts stay in PostgreSQL/artifact storage.

## Idempotency

Every external or cross-worker event must have a deterministic `dedupe_key`.

Examples:

```text
telegram:update:<update_id>
schedule:<schedule_id>:<planned_fire_at>
task:<task_id>:completed:<attempt>
delivery:<route_id>:<output_event_id>
```

Rules:

- duplicate inbound event returns existing event status;
- duplicate routed event does not create a second session input;
- duplicate output event does not send the same Telegram message twice for one route cursor;
- dedupe is enforced in PostgreSQL, not only in JetStream consumers.

## Ack Rules

- Webhook returns `200` only after the inbound event is durably stored and either published to JetStream or recorded in `event_outbox`.
- Router acks JetStream only after `routed_events` is written and the session input event is published or recorded in outbox.
- Agent worker acks only after the canonical run state is durable.
- Delivery worker acks only after delivery status is durable.
- Poison events go to `TEAMD_DLQ` with reason and original event reference.

## Error Handling

Retryable failures:

- transient NATS publish error;
- transient Telegram send error;
- temporary PostgreSQL connection error;
- worker shutdown before ack.

Non-retryable failures:

- invalid webhook secret;
- invalid event payload;
- no matching route and no default route;
- unauthorized operator/source;
- invalid result contract from delegated task.

Policies:

- retryable events use exponential backoff with `max_attempts`;
- non-retryable events go to `failed` or `dead_lettered` with a reason;
- DLQ events retain original `event_id`, `trace_id`, `source_id` and payload ref;
- delivery failure never rolls back a completed agent run;
- route failure never deletes the inbound event.

## Delegation, Dependencies And Chains

Delegation creates `task_registry` entries and publishes task events.

Parent execution must not block a worker thread while another agent runs. Dependencies are represented by:

- `dependency_json` in `task_registry`;
- `task_completed` events routed back into the parent session;
- explicit bounded wait tools only when the model needs the result immediately.

Chain metadata is mandatory for agent-agent hops:

- `chain_id`;
- `hop_count`;
- `max_hops`.

`grant_agent_chain_continuation` remains the explicit operator/runtime mechanism for increasing chain budget. Any worker that receives an event above `max_hops` must reject it as non-retryable and persist the error.

## Context Isolation

Agent-agent and subagent tasks do not receive the parent transcript by default.

Allowed transfer:

- `goal`;
- `constraints`;
- `expected_output`;
- `bounded_context`;
- `context_refs`;
- `artifact_refs`;
- `allowed_tools`;
- `write_scope`;
- `return_format`.

For subagent code/file work, `write_scope` is required. Missing write scope is a validation error.

Secrets and operator-private files are never copied into delegated context unless an explicit policy permits it.

## Router Rules

Rule evaluation order:

1. Explicit overrides.
2. Chat/group/topic rules.
3. Operator rules.
4. Source rules.
5. Global defaults.

Configurable fields:

- source type and address;
- authorized operator ids/roles;
- default `agent_id`;
- session strategy;
- queue policy;
- priority;
- output target ids;
- format policy;
- quiet hours and rate limits;
- allowed/denied tools;
- observability labels.

Session strategies:

- `per_private_chat`
- `per_group`
- `per_topic`
- `per_agent`
- `explicit_session`

Queue policies:

- `fifo`
- `priority`
- `coalesce`
- `restart`
- `reject`

Priority mechanics:

- implementation must choose and document whether lower or higher numeric priority wins;
- ties are broken by `received_at`, then deterministic `event_id`;
- critical monitoring alerts must be able to bypass routine chat messages.

## Deployment

The deploy stack must include:

- `nats-server -js`
- Caddy route for Telegram webhook:

```text
https://<domain>/v1/telegram/webhook/<secret>
```

`agentd` config must include:

- NATS URL;
- JetStream stream names;
- Telegram webhook public URL;
- Telegram webhook secret;
- webhook mode enabled;
- long polling disabled.

## Observability

Every event should carry:

- `trace_id`
- `event_id`
- `source_kind`
- `source_id`
- `session_id` when resolved
- `agent_id` when resolved
- `route_rule_id` when matched

Trace shape:

```text
telegram.webhook.receive
  -> event.persist
  -> nats.publish input
  -> router.resolve
  -> nats.publish session.input
  -> agent.run
  -> nats.publish session.output
  -> delivery.telegram.send
```

## Migration Strategy

This is a breaking architecture path for the next runtime version, not a hotfix.

Implementation should happen in feature branch/worktree. Production remains on the current stable runtime until the full chain is tested.

Suggested phases:

1. Add config/schema/NATS client and health checks.
2. Add webhook ingress that writes durable inbound events and publishes to JetStream.
3. Add router rules and router worker.
4. Add session input worker using the canonical chat path.
5. Add output/delivery worker.
6. Switch deploy stack from polling to webhook.
7. Remove Telegram long polling worker after webhook runtime is stable.

Compatibility rule during migration: there may be two transport adapters temporarily, but there must never be two session/runtime chat paths.
