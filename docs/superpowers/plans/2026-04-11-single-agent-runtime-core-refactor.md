# Single-Agent Runtime Core Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Commit the current working bot baseline from the active worktree into the main project, then refactor it into a readable single-agent runtime core with full functional parity and beginner-grade documentation for how the agent works.

**Architecture:** Keep all working behavior, but reorganize the code so the default mental model is a single-agent Telegram bot with explicit layers: bootstrap, transport, runtime, provider, tools, memory, compaction, launcher, and observability. Preserve mesh code as an isolated subsystem that is disabled by default and does not pollute the primary reading path.

**Tech Stack:** Go, Telegram Bot API, PostgreSQL, pgvector, Ollama (`nomic-embed-text`), systemd user services, beads issue tracking

---

## Target Outcome

At the end of this plan:

- The **main project** contains the full working baseline currently living in the active worktree at `/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD`.
- The bot is readable to a new engineer by following a small number of top-level modules.
- The default runtime path is **single-agent only**.
- Mesh remains in the repository, but is **clearly isolated and disabled by default**.
- Every key mechanism has beginner documentation:
  - bootstrap/config
  - sessions and runs
  - tool loop
  - memory and recall
  - compaction
  - runtime persistence
  - traces/status card
  - launcher and operations
  - “how to build your own agent”

## Source Baseline

**Current working source of truth:**

- `/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/go.mod`
- `/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/cmd/coordinator/main.go`
- `/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/internal/...`

**Current live operational workspaces:**

- `/home/administrator/teamD`
- `/home/administrator/teamD-helper`

## File Structure

### Existing target structure to preserve and clarify

- `cmd/coordinator/main.go`
  - Thin bootstrap only. No business logic beyond wiring.
- `internal/config/`
  - Env/config parsing only.
- `internal/transport/telegram/`
  - Telegram update ingestion, slash commands, status card rendering, reply formatting.
- `internal/runtime/`
  - Run manager, active registry, persistence integration, cancellation, restart recovery.
- `internal/provider/`
  - LLM provider contracts and ZAI client.
- `internal/mcp/`
  - Tool runtime and tool registration contracts.
- `internal/memory/`
  - Session memory, document memory, recall, embeddings.
- `internal/compaction/`
  - Compaction budgets, prompt assembly, checkpoint synthesis.
- `internal/llmtrace/`
  - Provider trace capture and persistence.
- `internal/skills/`
  - Skills discovery, activation, prompt fragments, tools.
- `internal/events/`
  - Event types and event bus used by runtime flows.
- `internal/observability/`
  - Logging and tracing helpers.
- `internal/approvals/`
  - Approval contracts and policy surfaces.
- `internal/artifacts/`
  - Artifact storage for copied evidence and generated files.
- `internal/workspace/`
  - Workspace context discovery and AGENTS.md loading.
- `internal/worker/`
  - Worker/checkpoint/runtime types still referenced by core flow.
- `internal/coordinator/`
  - Legacy orchestration and service code that must be documented even if reduced in the main path.
- `internal/mesh/`
  - Preserved but isolated and disabled by default.
- `scripts/teamd-agentctl`
  - Operational launcher.

### New documentation structure to add

- `docs/agent/01-overview.md`
- `docs/agent/02-bootstrap-and-config.md`
- `docs/agent/03-sessions-runs-and-cancellation.md`
- `docs/agent/04-tool-loop.md`
- `docs/agent/05-memory-and-recall.md`
- `docs/agent/06-compaction.md`
- `docs/agent/07-traces-status-and-observability.md`
- `docs/agent/08-launcher-and-ops.md`
- `docs/agent/09-build-your-own-agent.md`
- `docs/agent/10-supported-but-not-primary-modules.md`

### New optional code-facing documentation helpers

- `docs/agent/code-map.md`
  - One-page map from concepts to exact files.
- `docs/agent/request-lifecycle.md`
  - Step-by-step flow from Telegram update to final reply.

## Refactor Rules

- Do **not** drop working functionality for readability.
- Do **not** rewrite the bot from scratch unless a local subsystem is simpler to replace than to untangle.
- Treat the worktree implementation as the baseline to preserve.
- Prefer **moving and splitting** code over inventing new abstractions without need.
- Keep mesh code, but make sure a newcomer can ignore it and still understand the main bot.
- Add docs alongside code changes, not after.
- Every moved mechanism must leave behind:
  - a clearer boundary
  - tests still passing
  - a documentation entry explaining what moved and why

## Task 1: Commit the worktree baseline into the main project

**Files:**
- Copy from: `/home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/*`
- Modify: target main project files under `/home/admin/AI-AGENT/data/projects/teamD`

- [ ] **Step 1: Diff main project against the active worktree**

Run:
```bash
diff -ruN \
  /home/admin/AI-AGENT/data/projects/teamD \
  /home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD \
  | sed -n '1,240p'
```

Expected: a reviewable baseline diff showing code, docs, and scripts that exist only in the worktree.

- [ ] **Step 2: Exclude live-only workspace files from the merge**

Verify the baseline merge does **not** blindly import live runtime artifacts such as:

- `/home/administrator/teamD/.env`
- `/home/administrator/teamD/agent.pid`
- `/home/administrator/teamD/agent.log`
- `/home/administrator/teamD/var/*`

Expected: merge scope is code/docs/scripts, not machine-local runtime state.

- [ ] **Step 3: Copy the worktree implementation into the main project**
- [ ] **Step 3: Copy the worktree implementation into the main project**

Run with a copy method that preserves hidden files intentionally, for example:
```bash
rsync -a \
  --exclude '.git' \
  --exclude 'agent.pid' \
  --exclude 'agent.log' \
  --exclude 'var/' \
  --exclude '.env' \
  /home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD/ \
  /home/admin/AI-AGENT/data/projects/teamD/
```

Expected: main project now contains the working baseline code.

- [ ] **Step 4: Verify post-copy parity against the source worktree**

Run:
```bash
diff -ruN \
  --exclude '.git' \
  --exclude 'agent.pid' \
  --exclude 'agent.log' \
  --exclude 'var' \
  --exclude '.env' \
  /home/admin/AI-AGENT/data/projects/teamD \
  /home/admin/AI-AGENT/data/projects/teamD/.worktrees/teamD-runtime-core-mvp-1/data/projects/teamD \
  | sed -n '1,240p'
```

Expected: remaining diff is understood and intentional, not silent omission.

- [ ] **Step 5: Verify the copied tree contains the expected runtime modules**

Run:
```bash
find cmd internal -maxdepth 2 -type f | sort | sed -n '1,240p'
```

Expected: `cmd/coordinator`, `internal/runtime`, `internal/memory`, `internal/compaction`, `internal/transport/telegram`, `internal/llmtrace`, `internal/skills`, `internal/provider`, and `internal/mesh`.

- [ ] **Step 6: Run the full test suite on the main project baseline**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./...
```

Expected: all tests pass before any readability refactor starts.

- [ ] **Step 7: Commit the baseline import**

Run:
```bash
git add .
git commit -m "feat: import working single-agent runtime baseline from worktree"
```

## Task 2: Make mesh isolated and off the main reading path

**Files:**
- Modify: `cmd/coordinator/main.go`
- Modify: `internal/config/config.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/mesh/*`
- Create: `docs/agent/mesh-boundary.md`

- [ ] **Step 1: Identify every direct mesh dependency in the default startup path**

Run:
```bash
rg -n "mesh|Mesh|TEAMD_MESH|New.*Mesh|internal/mesh" \
  cmd/coordinator/main.go internal/config internal/transport/telegram
```

Expected: exact list of places where mesh leaks into the default single-agent flow.

- [ ] **Step 2: Introduce an explicit “mesh disabled by default” bootstrap branch**

Implementation goal:
- default boot path builds single-agent services only
- mesh services are only wired when config explicitly enables them

- [ ] **Step 3: Move mesh-specific wiring behind a dedicated helper**

Example target helpers:
- `wireSingleAgent(...)`
- `wireMeshSubsystem(...)`

Expected: `cmd/coordinator/main.go` becomes readable at a glance.

- [ ] **Step 4: Ensure Telegram direct mode does not require mesh concepts to be understood**

Expected: a newcomer reading the Telegram adapter can follow:
- update
- session
- run
- tool loop
- memory
- reply

without reading `internal/mesh`.

- [ ] **Step 5: Add a short mesh boundary document**

Document:
- what mesh is
- why it is preserved
- why it is disabled by default
- which packages belong to mesh and can be ignored for single-agent understanding

- [ ] **Step 6: Run tests for config/bootstrap/telegram paths**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./cmd/coordinator ./internal/config ./internal/transport/telegram
```

- [ ] **Step 7: Commit**

Run:
```bash
git add cmd/coordinator internal/config internal/transport/telegram internal/mesh docs/agent
git commit -m "refactor: isolate mesh from default single-agent runtime"
```

## Task 2.5: Inventory and classify supported subsystems that are not in the primary reading path

**Files:**
- Create: `docs/agent/10-supported-but-not-primary-modules.md`
- Modify: `docs/agent/code-map.md`

- [ ] **Step 1: Enumerate non-primary but still supported packages**

Run:
```bash
find internal -maxdepth 2 -type d | sort
```

Expected: explicit inventory including `events`, `observability`, `approvals`, `artifacts`, `workspace`, `worker`, and `coordinator`.

- [ ] **Step 2: Classify each package**

For each package, document one of:
- primary path
- supporting infrastructure
- legacy but supported
- mesh-only

- [ ] **Step 3: Link every such package in the code map**

Expected: no working subsystem is left undocumented just because it is not on the happy path.

- [ ] **Step 4: Commit**

Run:
```bash
git add docs/agent
git commit -m "docs: classify non-primary but supported runtime modules"
```

## Task 3: Split bootstrap from runtime so the entrypoint is readable

**Files:**
- Modify: `cmd/coordinator/main.go`
- Create or modify: `internal/runtime/run_manager.go`
- Create or modify: `internal/runtime/active_registry.go`
- Create: `internal/runtime/bootstrap.go` if useful
- Test: `internal/runtime/run_manager_test.go`

- [ ] **Step 1: Reduce `main.go` to config load, dependency construction, and service startup**

Expected shape:
- load config
- build logger/store/provider/tools/memory/transport
- start transport

- [ ] **Step 2: Move any remaining lifecycle logic out of `main.go`**

Expected: no request-loop or business-flow logic remains in bootstrap.

- [ ] **Step 3: Make `RunManager` the explicit home of run execution**

Expected responsibilities:
- start run
- track round count
- orchestrate provider/tool loop
- handle cancel
- finalize run state

- [ ] **Step 4: Ensure `ActiveRegistry` is the single place for in-flight run ownership**

Expected: Telegram transport should not own run orchestration details directly.

- [ ] **Step 5: Add or tighten tests for lifecycle boundaries**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime -run 'TestRunManager|TestActive'
```

- [ ] **Step 6: Commit**

Run:
```bash
git add cmd/coordinator internal/runtime
git commit -m "refactor: separate bootstrap from run lifecycle"
```

## Task 4: Shrink the Telegram adapter into a transport layer

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/run_state.go`
- Modify: `internal/transport/telegram/status_card.go`
- Modify: `internal/transport/telegram/formatting.go`
- Test: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Inventory the responsibilities currently in `adapter.go`**

Expected buckets:
- update dispatch
- command parsing
- run start/cancel routing
- prompt construction glue
- tool loop glue
- memory recall glue
- status card updates
- formatting

- [ ] **Step 2: Move non-transport responsibilities behind interfaces/helpers**

Targets:
- transport handles Telegram I/O
- runtime handles execution
- memory handles recall
- compaction handles summaries
- formatter handles only output formatting

- [ ] **Step 3: Keep slash-command behavior unchanged**

Must preserve:
- `/reset`
- `/session`
- `/mesh`
- `/status`
- `/runtime`
- `/model ...`
- `/params ...`
- `/skills ...`
- `/cancel`

- [ ] **Step 4: Make status-card updates clearly best-effort**

Expected: status card errors do not look like runtime failures in code structure.

- [ ] **Step 5: Run Telegram test suite**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram
```

- [ ] **Step 6: Commit**

Run:
```bash
git add internal/transport/telegram
git commit -m "refactor: reduce telegram adapter to transport concerns"
```

## Task 5: Make the tool loop readable and explicit

**Files:**
- Modify: `internal/runtime/run_manager.go`
- Modify: `internal/provider/provider.go`
- Modify: `internal/provider/zai/client.go`
- Modify: `internal/mcp/runtime.go`
- Create: `docs/agent/04-tool-loop.md`

- [ ] **Step 1: Document the current tool loop in code comments and docs**

Explain:
- provider request
- tool call response
- tool execution
- tool result append
- next round
- final assistant reply

- [ ] **Step 2: Make one function own the round loop**

Expected: a newcomer can find one obvious function that answers “how does the model use tools?”

- [ ] **Step 3: Keep guardrails explicit in that loop**

Must remain visible:
- round timeout
- repeated tool call breaker
- advisory stop policy
- cancellation checks

- [ ] **Step 4: Add a request-lifecycle diagram to docs**

Show:
- Telegram update
- run creation
- prompt assembly
- provider round
- tool execution
- reply send

- [ ] **Step 5: Run runtime/provider/mcp tests**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./internal/runtime ./internal/provider/... ./internal/mcp
```

- [ ] **Step 6: Commit**

Run:
```bash
git add internal/runtime internal/provider internal/mcp docs/agent
git commit -m "docs: clarify tool loop and request lifecycle"
```

## Task 6: Make memory and compaction understandable

**Files:**
- Modify: `internal/memory/*`
- Modify: `internal/compaction/*`
- Modify: `internal/runtime/postgres_store.go`
- Create: `docs/agent/05-memory-and-recall.md`
- Create: `docs/agent/06-compaction.md`

- [ ] **Step 1: Separate and name the memory layers clearly**

Docs and code should distinguish:
- raw session history
- checkpoint
- continuity
- memory documents
- recall
- embeddings/vector search

- [ ] **Step 2: Make compaction trigger/config easy to find**

Document exact env/config:
- `TEAMD_CONTEXT_WINDOW_TOKENS`
- `TEAMD_PROMPT_BUDGET_TOKENS`
- `TEAMD_COMPACTION_TRIGGER_TOKENS`
- `TEAMD_MAX_TOOL_CONTEXT_CHARS`
- `TEAMD_LLM_COMPACTION_ENABLED`
- `TEAMD_LLM_COMPACTION_TIMEOUT`

- [ ] **Step 3: Put “how memory gets written” in one documented flow**

Must explain:
- when checkpoint is created
- when continuity is updated
- when a checkpoint is promoted to memory document
- when embeddings are generated
- how recall is injected or searched

- [ ] **Step 4: Ensure noisy/transient content rules are obvious**

Expected: a newcomer can find why search snippets or binary/tool dumps do not belong in long-term memory.

- [ ] **Step 5: Add a “how to test memory” section**

Include:
- a manual Telegram scenario
- DB inspection commands
- trace inspection steps

- [ ] **Step 6: Run memory/compaction tests**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./internal/memory ./internal/compaction
```

- [ ] **Step 7: Commit**

Run:
```bash
git add internal/memory internal/compaction internal/runtime docs/agent
git commit -m "docs: clarify memory and compaction architecture"
```

## Task 7: Make launcher and operations beginner-safe

**Files:**
- Modify: `scripts/teamd-agentctl`
- Create: `docs/agent/08-launcher-and-ops.md`

- [ ] **Step 1: Keep `teamd-agentctl` as the only documented launcher**

Document commands:
- `install`
- `uninstall`
- `start`
- `stop`
- `restart`
- `status`
- `logs`

- [ ] **Step 2: Document the live workspace contract**

Explain:
- what must exist in `/home/administrator/teamD*`
- what `.env` is expected to provide
- where `agent.pid` comes from
- why systemd user services are used

- [ ] **Step 3: Add failure diagnosis steps**

Must include:
- `systemctl --user status ...`
- `journalctl --user -u ...`
- listener checks
- stale process cleanup behavior

- [ ] **Step 4: Verify launcher commands against live services**

Run:
```bash
./scripts/teamd-agentctl status teamd-main
./scripts/teamd-agentctl status teamd-helper
```

- [ ] **Step 5: Commit**

Run:
```bash
git add scripts/teamd-agentctl docs/agent
git commit -m "docs: add launcher and operations guide"
```

## Task 8: Write the newcomer guide for building a similar agent

**Files:**
- Create: `docs/agent/01-overview.md`
- Create: `docs/agent/02-bootstrap-and-config.md`
- Create: `docs/agent/03-sessions-runs-and-cancellation.md`
- Create: `docs/agent/04-tool-loop.md`
- Create: `docs/agent/05-memory-and-recall.md`
- Create: `docs/agent/06-compaction.md`
- Create: `docs/agent/07-traces-status-and-observability.md`
- Create: `docs/agent/09-build-your-own-agent.md`
- Create: `docs/agent/code-map.md`
- Create: `docs/agent/10-supported-but-not-primary-modules.md`

- [ ] **Step 1: Write the top-level overview**

Must answer:
- what this bot is
- what the main request path is
- which files matter first

- [ ] **Step 2: Write “sessions and runs” for complete beginners**

Use plain language:
- Telegram update
- session
- run
- round
- cancellation
- restart recovery

- [ ] **Step 3: Write “tool loop” as a tutorial, not just architecture notes**

Include one worked example of:
- user asks something
- model requests a tool
- tool returns output
- model answers

- [ ] **Step 4: Write “memory and compaction” with concrete examples**

Include:
- what is remembered
- what is not
- how recall reaches the model
- why compaction exists

- [ ] **Step 5: Write “build your own agent” as a recipe**

Must include the minimum set of components:
- transport
- runtime loop
- provider
- tools
- memory
- launcher

- [ ] **Step 6: Link every doc to exact code paths**

Expected: every major concept points to the Go files that implement it.

- [ ] **Step 7: Commit**

Run:
```bash
git add docs/agent
git commit -m "docs: add beginner guide for single-agent runtime core"
```

## Task 9: Final parity verification

**Files:**
- Verify current runtime code and docs

- [ ] **Step 1: Run the full Go test suite**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./...
```

- [ ] **Step 2: Verify launcher still controls both live bots**

Run a non-live control-path check first:
```bash
./scripts/teamd-agentctl status teamd-main
./scripts/teamd-agentctl status teamd-helper
./scripts/teamd-agentctl logs teamd-main --lines 20
./scripts/teamd-agentctl logs teamd-helper --lines 20
```

Expected: launcher commands work even before any live Telegram interaction is used as confirmation.

- [ ] **Step 3: Verify isolated runtime paths without relying on live Telegram**

Run:
```bash
GOTMPDIR=$PWD/.tmp/go go test ./cmd/coordinator ./internal/runtime ./internal/transport/telegram ./internal/memory ./internal/compaction
```

Expected: core paths verify without requiring live bot traffic.

- [ ] **Step 4: Run a manual functional parity smoke test**

Manual checks:
- Telegram bot replies normally
- `/reset` works
- `/session` works
- `/runtime` works
- `/model set ...` works
- `/params set ...` works
- `/skills` works
- `/status` works
- `/cancel` works
- `/skills list` works
- `/mesh` still reports disabled/default behavior correctly
- memory recall still works
- trace files still appear under `var/llm-traces`
- status-card behavior still updates sanely during a tool run
- reply formatting still preserves mixed prose + non-tabular content

- [ ] **Step 5: Verify docs are sufficient for a newcomer**

Checklist:
- newcomer can find request entrypoint
- newcomer can find tool loop
- newcomer can find compaction trigger
- newcomer can find memory write path
- newcomer can find launcher and logs

- [ ] **Step 6: Close the beads issue if all acceptance criteria are met**

Run:
```bash
bd close teamD-zri
```

- [ ] **Step 7: Final commit**

Run:
```bash
git add .
git commit -m "refactor: document and clarify single-agent runtime core"
```

## Acceptance Criteria

- The worktree baseline is committed into the main project.
- The default bot reading path no longer requires understanding mesh.
- `cmd/coordinator/main.go` is thin and readable.
- Telegram adapter is transport-focused rather than a god-object.
- The tool loop has one obvious owner and clear docs.
- Memory, recall, and compaction are documented for beginners with exact code links.
- Launcher and operational flow are documented and reproducible.
- Both non-live and live verification paths are documented.
- Mesh remains present, but isolated and disabled by default.
- Current live functionality is preserved.
- Preserved slash-command behavior is verified as a matrix, not by a narrow smoke test.
- Extra supported subsystems outside the primary reading path are still documented.
- A new engineer can read `docs/agent/*` and understand how to build a similar agent.

## Notes

- Do not attempt a big-bang rewrite.
- If a subsystem becomes simpler to replace than to untangle, replace only that subsystem and document the decision.
- If any working behavior is discovered outside the current scope, expand the documentation and module boundaries rather than silently dropping it.
