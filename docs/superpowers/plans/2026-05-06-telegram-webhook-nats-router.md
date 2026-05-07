# Telegram Webhook NATS Router Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Telegram long polling with a durable webhook -> NATS JetStream -> rule router -> session worker -> delivery worker pipeline.

**Architecture:** NATS JetStream is a mandatory runtime dependency for this event-driven path. PostgreSQL remains the source of truth for events, sessions, routing config, transcripts and delivery state. The webhook, router and workers must call the canonical App/runtime path and must not introduce a second chat loop.

**Tech Stack:** Rust, `agentd` HTTP server, PostgreSQL persistence, NATS JetStream, Caddy, Telegram Bot API webhook, existing App/runtime/chat execution.

---

## Design Baseline

Read first:

- `docs/superpowers/specs/2026-05-06-telegram-webhook-nats-router-design.md`
- `docs/current/19-mimo-agent-mesh.md`
- `docs/current/17-runtime-mental-model.md`
- `docs/current/13-observability-tracing-plan.md`

## File Map

Expected files to create or modify:

- Modify `crates/agent-persistence/src/config.rs`: add mandatory NATS and Telegram webhook config.
- Modify `crates/agent-persistence/src/store/schema.rs`: add event/router/outbox tables.
- Modify `crates/agent-persistence/src/records.rs`: add event/router records.
- Modify `crates/agent-persistence/src/repository.rs`: add repository traits.
- Create `crates/agent-persistence/src/store/event_repos.rs`: inbound/routed/outbox/delivery repository implementation.
- Create `crates/agent-persistence/src/store/router_repos.rs`: router rule repository implementation.
- Create `crates/agent-persistence/src/store/task_registry_repos.rs`: async task registry implementation.
- Modify `crates/agent-persistence/src/store.rs`: export new repositories.
- Create `cmd/agentd/src/nats.rs`: NATS JetStream connection, stream bootstrap, publish/subscribe helpers.
- Create `cmd/agentd/src/event_bus.rs`: canonical event envelopes and publishing interfaces.
- Create `cmd/agentd/src/telegram/webhook.rs`: Telegram webhook parsing and ingress handler.
- Modify `cmd/agentd/src/http/router.rs` or equivalent HTTP route module: add webhook endpoint.
- Create `cmd/agentd/src/router_worker.rs`: rule-based router consumer with priority and DLQ behavior.
- Create `cmd/agentd/src/session_worker.rs`: session input consumer that invokes canonical chat execution.
- Create `cmd/agentd/src/delivery_worker.rs`: output consumer that sends Telegram delivery.
- Modify `cmd/agentd/src/telegram.rs`: disable long polling in webhook mode and start webhook/event workers.
- Modify deploy scripts/Caddy templates: add NATS container/service and webhook route.
- Add tests under `cmd/agentd/tests/` and `crates/agent-persistence/src/store/tests/`.
- Update `docs/current/09-operator-cheatsheet.md`, `docs/current/19-mimo-agent-mesh.md`, deploy docs.

## Task 1: Mandatory NATS And Webhook Config

**Files:**

- Modify `crates/agent-persistence/src/config.rs`
- Test: existing config tests in `crates/agent-persistence/src/config.rs` or dedicated config test module

- [ ] **Step 1: Write failing config tests**

Test cases:

- `event_bus.enabled=true` requires `nats.url`.
- `telegram.mode=webhook` requires `telegram.webhook_public_url` and `telegram.webhook_secret`.
- `telegram.mode=webhook` rejects long polling-only settings as active mode.
- `telegram.mode=polling` is no longer allowed when `event_bus.required=true`.

Run:

```bash
cargo test -p agent-persistence config_requires_nats_for_event_runtime
```

Expected before implementation: compile failure or validation failure.

- [ ] **Step 2: Add config structs**

Add explicit config sections:

```text
[event_bus]
required = true
backend = "nats_jetstream"
nats_url = "nats://127.0.0.1:4222"
input_stream = "TEAMD_INPUT"
session_stream = "TEAMD_SESSION"
delivery_stream = "TEAMD_DELIVERY"
task_stream = "TEAMD_TASKS"
dlq_stream = "TEAMD_DLQ"

[telegram]
mode = "webhook"
webhook_public_url = "https://teamd.example/v1/telegram/webhook/<secret>"
webhook_secret = "..."
```

- [ ] **Step 3: Implement validation**

Validation must reject event-driven Telegram mode without NATS.

- [ ] **Step 4: Run tests**

```bash
cargo test -p agent-persistence config_requires_nats_for_event_runtime
```

- [ ] **Step 5: Commit**

```bash
git add crates/agent-persistence/src/config.rs
git commit -m "feat: require nats for webhook event runtime"
```

## Task 2: Event And Router Persistence

**Files:**

- Modify `crates/agent-persistence/src/store/schema.rs`
- Modify `crates/agent-persistence/src/records.rs`
- Modify `crates/agent-persistence/src/repository.rs`
- Create `crates/agent-persistence/src/store/event_repos.rs`
- Create `crates/agent-persistence/src/store/router_repos.rs`
- Create `crates/agent-persistence/src/store/task_registry_repos.rs`
- Modify `crates/agent-persistence/src/store.rs`
- Tests: `crates/agent-persistence/src/store/tests/event_bus.rs`

- [ ] **Step 1: Write failing repository tests**

Test cases:

- inbound event insert is idempotent by `dedupe_key`;
- router rules list in deterministic priority order;
- routed event round-trips by id;
- outbox event can be claimed and marked published;
- delivery event status can be updated.
- task registry records agent_task/delegate status, dependencies, retry policy, chain metadata and result refs.

Run:

```bash
cargo test -p agent-persistence event_bus_repository
```

- [ ] **Step 2: Add schema**

Add tables:

- `event_sources`
- `router_rules`
- `inbound_events`
- `routed_events`
- `event_outbox`
- `event_deliveries`
- `task_registry`

Use the field set from the design spec.

- [ ] **Step 3: Add records and repository traits**

Keep records as DTOs. Validate ids with existing identifier validation helpers.

- [ ] **Step 4: Implement PostgreSQL repositories**

Follow existing store repository patterns. Avoid business logic in persistence.

- [ ] **Step 5: Run tests**

```bash
cargo test -p agent-persistence event_bus_repository
```

- [ ] **Step 6: Commit**

```bash
git add crates/agent-persistence
git commit -m "feat: add event routing persistence"
```

## Task 3: NATS JetStream Client

**Files:**

- Create `cmd/agentd/src/nats.rs`
- Create `cmd/agentd/src/event_bus.rs`
- Modify `cmd/agentd/src/main.rs` or bootstrap module exports as needed
- Tests: `cmd/agentd/tests/nats_event_bus.rs`

- [ ] **Step 1: Write failing unit tests with fake publisher**

Do not require a real NATS server for all tests. Define a small event publisher trait and test envelope formation.

Test cases:

- stream subjects are computed correctly;
- message envelope includes `event_id`, `trace_id`, `source_kind`, `source_id`, `payload_ref`;
- publish errors are surfaced without marking outbox published.
- DLQ envelope includes original event ref and failure reason.

- [ ] **Step 2: Add NATS client wrapper**

Use an async NATS crate with JetStream support. The wrapper must:

- connect to configured NATS URL;
- create/verify required streams;
- publish JSON envelopes;
- publish DLQ envelopes;
- expose consumer subscription helpers.

- [ ] **Step 3: Add optional integration test gated by env**

Run only when `TEAMD_TEST_NATS_URL` is set.

```bash
TEAMD_TEST_NATS_URL=nats://127.0.0.1:4222 cargo test -p agentd nats_jetstream_integration
```

- [ ] **Step 4: Run non-NATS tests**

```bash
cargo test -p agentd nats_event_bus
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/nats.rs cmd/agentd/src/event_bus.rs cmd/agentd/tests/nats_event_bus.rs
git commit -m "feat: add nats jetstream event bus"
```

## Task 3.5: Error Policy And Dead Letter Semantics

**Files:**

- Create `cmd/agentd/src/event_errors.rs`
- Modify `cmd/agentd/src/event_bus.rs`
- Tests: `cmd/agentd/tests/event_error_policy.rs`

- [ ] **Step 1: Write failing policy tests**

Test cases:

- transient NATS/Telegram/Postgres errors are retryable;
- invalid webhook secret, invalid payload and unauthorized source are non-retryable;
- exceeding `max_attempts` produces a DLQ event;
- DLQ event preserves `event_id`, `trace_id`, `source_id`, payload ref and reason.

- [ ] **Step 2: Implement shared event error classification**

Workers must use one shared classification instead of ad-hoc string matching.

- [ ] **Step 3: Run tests**

```bash
cargo test -p agentd --test event_error_policy
```

- [ ] **Step 4: Commit**

```bash
git add cmd/agentd/src/event_errors.rs cmd/agentd/src/event_bus.rs cmd/agentd/tests/event_error_policy.rs
git commit -m "feat: add event runtime error policy"
```

## Task 4: Telegram Webhook Ingress

**Files:**

- Create `cmd/agentd/src/telegram/webhook.rs`
- Modify HTTP router module to mount `POST /v1/telegram/webhook/{secret}`
- Modify `cmd/agentd/src/telegram.rs`
- Tests: `cmd/agentd/tests/telegram_webhook.rs`

- [ ] **Step 1: Write failing webhook tests**

Test cases:

- wrong secret returns unauthorized/forbidden and does not persist event;
- valid update stores exactly one inbound event;
- repeated Telegram `update_id` is deduped;
- duplicate update returns success without creating a second run or second routed event;
- valid update publishes one JetStream envelope or outbox row;
- webhook handler does not execute a chat turn.

- [ ] **Step 2: Implement parser**

Extract:

- `update_id`
- chat id;
- thread id if present;
- Telegram user id;
- message text/document metadata;
- source kind.

Dedupe key:

```text
telegram:update:<update_id>
```

- [ ] **Step 3: Implement HTTP handler**

Handler sequence:

1. validate secret;
2. parse JSON;
3. persist inbound event;
4. publish or outbox event;
5. return `200`.

- [ ] **Step 4: Run tests**

```bash
cargo test -p agentd --test telegram_webhook
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/telegram cmd/agentd/src/http cmd/agentd/tests/telegram_webhook.rs
git commit -m "feat: add telegram webhook ingress"
```

## Task 5: Rule-Based Router Worker

**Files:**

- Create `cmd/agentd/src/router_worker.rs`
- Modify app/bootstrap exports if route resolution belongs there
- Tests: `cmd/agentd/tests/router_worker.rs`

- [ ] **Step 1: Write failing router tests**

Test cases:

- exact chat rule routes to configured agent/session strategy;
- operator rule applies when no chat rule matches;
- global default applies last;
- higher priority rule wins;
- priority tie breaks by `received_at`, then deterministic `event_id`;
- disabled rules are ignored;
- no matching route without default writes non-retryable route failure;
- routed event is persisted before publishing session input.

- [ ] **Step 2: Implement route resolution**

Keep matching deterministic and explainable. Return a route decision:

```text
session_id
agent_id
queue_policy
priority
output_targets
format_policy
tool_policy
retry_policy
labels
matched_rule_id
```

- [ ] **Step 3: Implement worker**

Consume `teamd.input.*`, load inbound event by payload ref, resolve route, persist `routed_events`, publish `teamd.session.<session_id>.input`.

For unauthorized/no-route/invalid-policy cases, persist failure and publish DLQ according to `event_errors`.

- [ ] **Step 4: Run tests**

```bash
cargo test -p agentd --test router_worker
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/router_worker.rs cmd/agentd/tests/router_worker.rs
git commit -m "feat: add rule based event router"
```

## Task 6: Session Input Agent Worker

**Files:**

- Create `cmd/agentd/src/session_worker.rs`
- Modify `cmd/agentd/src/bootstrap/execution_ops.rs` only if needed to expose canonical queued execution
- Tests: `cmd/agentd/tests/session_event_worker.rs`

- [ ] **Step 1: Write failing worker tests**

Test cases:

- consumes routed session input and executes canonical chat path;
- records user transcript from event payload;
- records assistant transcript from provider response;
- does not know Telegram chat id;
- publishes session output event after durable transcript write.
- creates/updates task registry entry for session work;
- explicit task dependencies can leave work in `waiting_dependency` instead of running immediately.

- [ ] **Step 2: Implement input conversion**

Convert routed event payload into the same input format used by current CLI/TUI/Telegram backend execution.

- [ ] **Step 3: Implement worker loop**

Ack only after durable run/transcript state and output event publish/outbox.

Do not pass parent agent transcripts into delegated task context unless the routed payload contains explicit `context_refs` or `bounded_context`.

- [ ] **Step 4: Run tests**

```bash
cargo test -p agentd --test session_event_worker
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/session_worker.rs cmd/agentd/tests/session_event_worker.rs
git commit -m "feat: execute routed session events"
```

## Task 7: Output Delivery Worker

**Files:**

- Create `cmd/agentd/src/delivery_worker.rs`
- Modify `cmd/agentd/src/telegram/router.rs` or extract reusable Telegram send helpers
- Tests: `cmd/agentd/tests/delivery_event_worker.rs`

- [ ] **Step 1: Write failing delivery tests**

Test cases:

- output event sends assistant text to configured Telegram target;
- route cursor prevents duplicate sends;
- delivery failure persists error and does not roll back run;
- `format_policy=status_only` does not send full text.

- [ ] **Step 2: Extract Telegram delivery helpers if needed**

Do not keep delivery logic trapped inside polling worker. Move reusable send/chunk/file functions behind a small interface.

- [ ] **Step 3: Implement delivery worker**

Consume `teamd.session.*.output`, load routes/targets, apply format/send policy, persist delivery status.

- [ ] **Step 4: Run tests**

```bash
cargo test -p agentd --test delivery_event_worker
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/delivery_worker.rs cmd/agentd/src/telegram cmd/agentd/tests/delivery_event_worker.rs
git commit -m "feat: deliver session output events"
```

## Task 8: Runtime Startup And Deployment Stack

**Files:**

- Modify `cmd/agentd/src/telegram.rs`
- Modify `cmd/agentd/src/bootstrap.rs` or daemon startup
- Modify deploy scripts under `scripts/`
- Modify Caddy/container stack files
- Tests: startup/config tests and smoke tests

- [ ] **Step 1: Write startup tests**

Test cases:

- webhook mode starts NATS/router/session/delivery workers;
- webhook mode does not start long polling;
- missing NATS fails startup clearly;
- health endpoint reports NATS status.

- [ ] **Step 2: Wire startup**

Start:

- NATS stream bootstrap;
- webhook HTTP route;
- router worker;
- session worker;
- delivery worker;
- outbox publisher if implemented.

- [ ] **Step 3: Update deploy stack**

Add:

- `nats-server -js`;
- Caddy webhook route;
- config/env variables;
- Telegram `setWebhook` helper command or deploy step.

- [ ] **Step 4: Run tests**

```bash
cargo test -p agentd webhook_mode_starts_event_workers
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd scripts docs
git commit -m "feat: wire webhook event runtime startup"
```

## Task 9: End-To-End Smoke Test

**Files:**

- Create `cmd/agentd/tests/event_runtime_smoke.rs`
- Update docs

- [ ] **Step 1: Write e2e smoke test with fakes**

Use fake Telegram backend and fake NATS if possible.

Flow:

```text
POST Telegram update
-> inbound event
-> route decision
-> session input
-> fake provider response
-> output event
-> fake Telegram send
```

Assertions:

- one user transcript;
- one assistant transcript;
- one delivery;
- trace ids present across events;
- duplicate webhook update does not duplicate run.
- task registry shows completed session work;
- route decision is explainable by matched rule id;
- DLQ remains empty for the happy path.

- [ ] **Step 2: Add optional real NATS e2e**

Gate with `TEAMD_TEST_NATS_URL`.

- [ ] **Step 3: Run quality gates**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build -p agentd
cargo build --release -p agentd
```

- [ ] **Step 4: Commit and push**

```bash
git status --short
git push
```

Do not deploy to production unless the operator explicitly asks for deployment.
