# MIMO Agent Mesh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Phase 1 of MIMO agent routing: named delivery targets, session output routes, async task/result semantics, and operator visibility without introducing a second chat loop or external bus.

**Architecture:** Keep `agentd` as modular monolith. PostgreSQL remains source of truth; the Phase 1 result bus is represented by jobs, inbox events, task registry views, delivery requests, and trace links. Telegram, CLI and TUI stay thin surfaces over the same App/runtime layer.

**Tech Stack:** Rust, `agent-runtime`, `agent-persistence`, `agentd` bootstrap/execution/telegram modules, PostgreSQL schema bootstrap, existing background worker, existing audit/trace model.

---

## Design Baseline

Read first:

- `docs/current/19-mimo-agent-mesh.md`
- `docs/current/05-interagent-background-and-schedules.md`
- `docs/current/13-observability-tracing-plan.md`
- `docs/current/17-runtime-mental-model.md`

External references are listed in `docs/current/19-mimo-agent-mesh.md`.

## File Map

Expected files to modify or create:

- Modify `crates/agent-persistence/src/store/schema.rs`: tables/indexes for delivery targets and output routes.
- Modify `crates/agent-persistence/src/records.rs`: records for `DeliveryTargetRecord` and `SessionOutputRouteRecord`.
- Modify `crates/agent-persistence/src/repository.rs`: repository traits.
- Create or extend `crates/agent-persistence/src/store/delivery_repos.rs`: storage implementation.
- Modify `crates/agent-persistence/src/store.rs` or module exports if needed.
- Modify `cmd/agentd/src/bootstrap.rs` and/or create `cmd/agentd/src/bootstrap/delivery_ops.rs`: App operations for delivery targets/routes.
- Modify `cmd/agentd/src/telegram/commands.rs`: `/target`, `/targets`, `/attach_output`, `/detach_output`, `/outputs`.
- Modify `cmd/agentd/src/telegram/router.rs`: command handling and route-aware pending delivery.
- Modify `cmd/agentd/src/telegram/render.rs`: rendering target/route lists.
- Modify `cmd/agentd/tests/telegram_surface.rs`: Telegram command and route delivery tests.
- Modify `crates/agent-persistence/src/store/tests/telegram.rs` or add a dedicated store test.
- Modify `docs/current/09-operator-cheatsheet.md`, `docs/current/15-tool-reference.md`, `docs/current/19-mimo-agent-mesh.md`.

Do not modify provider loop for the first delivery-target slice unless a test proves it is necessary.

## Task 1: Persistence Model For Delivery Targets

**Bead:** `teamD-mimo-delivery-targets`

**Files:**

- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Create/modify: `crates/agent-persistence/src/store/delivery_repos.rs`
- Test: `crates/agent-persistence/src/store/tests.rs` or `crates/agent-persistence/src/store/tests/delivery.rs`

- [ ] **Step 1: Write failing store round-trip test**

Test cases:

- `DeliveryTargetRecord` round-trips by `target_id`.
- List by kind/scope returns stable order.
- `SessionOutputRouteRecord` round-trips by `route_id`.
- List enabled routes for session returns only enabled routes.
- Route cursor update does not mutate the target.

Run:

```bash
cargo test -p agent-persistence delivery_target
```

Expected before implementation: compile failure or missing methods.

- [ ] **Step 2: Add schema**

Tables:

```sql
delivery_targets(
  target_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  address TEXT NOT NULL,
  scope TEXT NOT NULL,
  owner_user_id TEXT,
  allowed_agent_ids_json TEXT NOT NULL DEFAULT '[]',
  allowed_session_ids_json TEXT NOT NULL DEFAULT '[]',
  send_policy_json TEXT NOT NULL DEFAULT 'null',
  format_policy TEXT NOT NULL DEFAULT 'full_text',
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
)

session_output_routes(
  route_id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  target_id TEXT NOT NULL REFERENCES delivery_targets(target_id) ON DELETE CASCADE,
  filter_json TEXT NOT NULL DEFAULT 'null',
  format_policy TEXT NOT NULL DEFAULT 'full_text',
  enabled BOOLEAN NOT NULL DEFAULT TRUE,
  last_delivered_transcript_created_at BIGINT,
  last_delivered_transcript_id TEXT,
  created_at BIGINT NOT NULL,
  updated_at BIGINT NOT NULL
)
```

Indexes:

- `idx_delivery_targets_kind_scope`
- `idx_session_output_routes_session_enabled`
- `idx_session_output_routes_target_id`

- [ ] **Step 3: Add records and repository**

Keep records as plain persistence DTOs. Do not add runtime business logic here.

Validation:

- `target_id` and `route_id` use `validate_identifier`.
- `kind` for Phase 1 accepts at least `telegram`.
- `format_policy` accepts at least `full_text`, `summary`, `status_only`, `errors_only`.

- [ ] **Step 4: Make tests pass**

Run:

```bash
cargo test -p agent-persistence delivery
```

- [ ] **Step 5: Commit**

```bash
git add crates/agent-persistence
git commit -m "feat: add delivery target persistence"
```

## Task 2: App Operations For Targets And Routes

**Bead:** `teamD-mimo-delivery-targets`

**Files:**

- Modify: `cmd/agentd/src/bootstrap.rs`
- Create/modify: `cmd/agentd/src/bootstrap/delivery_ops.rs`
- Test: `cmd/agentd/tests/bootstrap_app.rs` or `cmd/agentd/tests/bootstrap_app/delivery.rs`

- [ ] **Step 1: Write failing App tests**

Test:

- create Telegram delivery target alias for chat id;
- list targets;
- attach current session to target;
- list output routes;
- detach route;
- reject duplicate alias only if it would point to a different address.

- [ ] **Step 2: Implement App API**

Operations:

```text
create_delivery_target(target_id, kind, address, scope, owner_user_id, now)
list_delivery_targets()
attach_session_output(session_id, target_id, format_policy, now)
detach_session_output(session_id, target_id, now)
list_session_outputs(session_id)
```

Keep this layer transport-neutral except validation for `kind=telegram`.

- [ ] **Step 3: Make tests pass**

Run:

```bash
cargo test -p agentd bootstrap_app::delivery
```

- [ ] **Step 4: Commit**

```bash
git add cmd/agentd/src/bootstrap* cmd/agentd/tests/bootstrap_app*
git commit -m "feat: add delivery target app operations"
```

## Task 3: Telegram Target Commands

**Bead:** `teamD-mimo-delivery-targets`

**Files:**

- Modify: `cmd/agentd/src/telegram/commands.rs`
- Modify: `cmd/agentd/src/telegram/render.rs`
- Modify: `cmd/agentd/src/telegram/router.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`
- Docs: `docs/current/09-operator-cheatsheet.md`

- [ ] **Step 1: Write failing command parser tests**

Commands:

```text
/target register <alias>
/targets
/attach_output <alias>
/detach_output <alias>
/outputs
```

Rules:

- `/target register <alias>` registers current Telegram chat as target.
- `/targets` lists known targets.
- `/attach_output <alias>` attaches selected session to target.
- `/outputs` lists routes for selected session.
- `/detach_output <alias>` disables/removes route.

- [ ] **Step 2: Write failing Telegram worker test**

Use `RecordingTelegramBackend` extensions or a real store-backed worker test.

Assertions:

- registered target response contains alias and current chat id;
- output route response contains selected session id and target alias;
- no model run is started for these commands;
- commands require activated pairing.

- [ ] **Step 3: Implement parser and router**

Do not add a second Telegram delivery loop. Commands call App operations.

- [ ] **Step 4: Make tests pass**

Run:

```bash
cargo test -p agentd telegram::commands::tests
cargo test -p agentd --test telegram_surface telegram_worker_routes_delivery_target_commands
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/telegram docs/current/09-operator-cheatsheet.md
git commit -m "feat: add telegram delivery target commands"
```

## Task 4: Route-Aware Assistant Transcript Delivery

**Bead:** `teamD-mimo-delivery-targets`

**Files:**

- Modify: `cmd/agentd/src/telegram/router.rs`
- Modify: `cmd/agentd/src/telegram/delivery.rs`
- Test: `cmd/agentd/tests/telegram_surface.rs`

- [ ] **Step 1: Write failing route delivery test**

Setup:

- chat A selected session `session-monitor`;
- chat B registered as `ops-status`;
- route `session-monitor -> ops-status`;
- assistant transcript is appended after route cursor.

Expected:

- `deliver_pending_session_notifications` sends transcript to chat B;
- route cursor advances;
- selected chat binding cursor is independent;
- repeated delivery does not duplicate.

- [ ] **Step 2: Implement route-aware delivery**

Extend pending delivery scan:

```text
for chat bindings: current behavior
for enabled session_output_routes where target.kind=telegram:
  load route cursor
  find assistant transcripts after route cursor
  apply format policy
  send to target address
  update route cursor
```

Do not change `deliver_chat_report` for the active chat until route delivery test passes.

- [ ] **Step 3: Handle delivery errors**

If target delivery fails:

- emit audit event;
- do not advance route cursor;
- do not fail unrelated chat binding delivery.

- [ ] **Step 4: Make tests pass**

Run:

```bash
cargo test -p agentd --test telegram_surface route_aware_pending_delivery
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src/telegram
git commit -m "feat: deliver session outputs to telegram targets"
```

## Task 5: Task Registry View Over Existing Jobs

**Bead:** `teamD-mimo-task-registry`

**Files:**

- Modify/create: `cmd/agentd/src/bootstrap/task_registry_ops.rs`
- Modify: `cmd/agentd/src/bootstrap.rs`
- Modify: `cmd/agentd/src/telegram/commands.rs`
- Modify: `cmd/agentd/src/telegram/router.rs`
- Modify: `cmd/agentd/src/cli/*`
- Test: `cmd/agentd/tests/bootstrap_app/*`, `cmd/agentd/tests/telegram_surface.rs`

- [ ] **Step 1: Write failing tests for task view**

Map existing records:

- `jobs.kind=interagent_message` -> `agent_task`;
- `jobs.kind=delegate` -> `delegate`;
- `jobs.kind=scheduled_chat_turn` -> `schedule_fire`;
- file delivery requests -> `delivery`.

- [ ] **Step 2: Implement read-only registry view**

Do not create a new source of truth yet. Build a projection over existing jobs and delivery requests.

- [ ] **Step 3: Add Telegram commands**

```text
/tasks
/task <task_id>
```

Use `/cancel <id>` only after cancellation semantics are proven.

- [ ] **Step 4: Make tests pass**

Run:

```bash
cargo test -p agentd task_registry
cargo test -p agentd --test telegram_surface telegram_worker_routes_task_registry_commands
```

- [ ] **Step 5: Commit**

```bash
git add cmd/agentd/src docs/current
git commit -m "feat: expose session task registry"
```

## Task 6: Contract Documentation And Tool Wording

**Bead:** `teamD-mimo-contracts`

**Files:**

- Modify: `docs/current/05-interagent-background-and-schedules.md`
- Modify: `docs/current/15-tool-reference.md`
- Modify: `docs/current/19-mimo-agent-mesh.md`
- Modify: `crates/agent-runtime/src/tool/catalog.rs`
- Modify: `crates/agent-runtime/src/tool/schema.rs`
- Test: `crates/agent-runtime/src/tool/tests.rs`

- [ ] **Step 1: Write failing tool wording tests**

Ensure descriptions say:

- `message_agent` queues async work and returns ids/task refs;
- use `session_wait` only when caller explicitly needs bounded snapshot now;
- parent should normally continue and inspect `/tasks`/task registry later;
- `delegate` is subagent worker semantics, not user-facing chat.

- [ ] **Step 2: Update docs and tool definitions**

Keep current tool names for compatibility. Add “future contract” wording only where it helps model behavior.

- [ ] **Step 3: Make tests pass**

Run:

```bash
cargo test -p agent-runtime tool_definition
```

- [ ] **Step 4: Commit**

```bash
git add crates/agent-runtime/src/tool docs/current
git commit -m "docs: clarify async agent task contracts"
```

## Task 7: Full Verification

**Files:** all touched files.

- [ ] **Step 1: Format**

```bash
cargo fmt --all
```

- [ ] **Step 2: Lint**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] **Step 3: Test**

```bash
cargo test --workspace --all-features
```

- [ ] **Step 4: Build**

```bash
cargo build -p agentd
cargo build --release -p agentd
```

- [ ] **Step 5: Update beads**

Close completed issues:

```bash
bd close teamD-mimo-contracts teamD-mimo-delivery-targets teamD-mimo-task-registry
```

Leave `teamD-mimo-routing` and `teamD-mimo-event-backbone` open unless implemented.

## Deployment Gate

Do not deploy automatically unless explicitly requested in the current turn.

If deploying:

1. Build release locally.
2. Copy only the binary or use existing deploy script.
3. Verify prod `/v1/status`.
4. Verify Telegram commands:

```text
/target register ops-status
/targets
/attach_output ops-status
/outputs
```

5. Create a short interval test schedule and confirm output arrives in the target chat.

