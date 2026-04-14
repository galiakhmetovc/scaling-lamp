# Agent Mesh Cold Start Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a local mesh of equal agents that start without capabilities, register themselves, collaboratively evaluate tasks in cold start, and gradually derive soft specialization from real outcomes.

**Architecture:** Each agent process exposes a peer API and registers itself in a shared registry. The first agent that receives a human task becomes the `owner` for that trace. During cold start, the owner samples one or more peer agents, compares candidate results using deterministic checks plus optional owner/judge scoring, and records outcome-based scores by task class. Capabilities are not declared up front; they emerge as soft preferences from accumulated wins, latency, and success rates. For the first slice the peer transport may stay HTTP for speed of implementation, but this is explicitly temporary and should stay compatible with a later move to `gRPC`.

**Tech Stack:** Go 1.24+, existing coordinator/runtime, Postgres for registry and score storage, local HTTP or gRPC peer transport, existing Telegram ingress for human-facing owner selection, Go tests.

---

## File Structure

- Create: `internal/mesh/types.go`
  Purpose: define peer descriptors, task envelopes, candidate replies, task classes, and score records.
- Create: `internal/mesh/registry.go`
  Purpose: registry interface for register/list/heartbeat/unregister peer agents.
- Create: `internal/mesh/postgres_registry.go`
  Purpose: Postgres-backed registry for online peers and soft specialization scores.
- Create: `internal/mesh/postgres_registry_test.go`
  Purpose: verify registration, heartbeat expiry semantics, and score persistence against real Postgres as integration coverage.
- Create: `internal/mesh/registry_test.go`
  Purpose: verify the registry contract quickly against an in-memory or mock implementation for fast unit feedback.
- Create: `internal/mesh/router.go`
  Purpose: owner-side peer selection logic for cold start, warm routing, and exploration vs exploitation, with structured decision logging.
- Create: `internal/mesh/router_test.go`
  Purpose: verify sampling policy, owner retention, and peer ranking from scores.
- Create: `internal/mesh/classifier.go`
  Purpose: run LLM-based task classification into the closed set of MVP task classes.
- Create: `internal/mesh/classifier_test.go`
  Purpose: verify strict parsing of classifier output and fallback behavior.
- Create: `internal/mesh/evaluator.go`
  Purpose: compare multiple candidate results and produce a winner plus score updates via `LLMJudge -> OwnerFallbackJudge`, with structured decision logging.
- Create: `internal/mesh/evaluator_test.go`
  Purpose: verify deterministic winner selection and fallback scoring rules.
- Create: `internal/mesh/http_transport.go`
  Purpose: expose and call peer APIs between agents on the same server.
- Create: `internal/mesh/http_transport_test.go`
  Purpose: verify message delivery and owner/reply envelope semantics.
- Modify: `cmd/coordinator/main.go`
  Purpose: start peer listener, register the current agent, and wire mesh components.
- Modify: `internal/config/config.go`
  Purpose: add mesh config such as agent id, listen address, registry DSN, cold-start fanout, exploration rate, peer timeout, heartbeat interval, and stale-peer threshold.
- Modify: `internal/transport/telegram/adapter.go`
  Purpose: treat the Telegram-facing agent as task owner and route mesh evaluation for selected task classes.
- Modify: `README.md`
  Purpose: document mesh cold start, peer registration, scoring, and owner semantics.

---

## Data Model

### Registry Row

`mesh_agents`
- `agent_id`
- `addr`
- `model`
- `status` (`idle|busy|draining`)
- `started_at`
- `last_seen_at`
- `metadata jsonb`

### Score Row

`mesh_agent_scores`
- `agent_id`
- `task_class`
- `tasks_seen`
- `tasks_won`
- `success_count`
- `failure_count`
- `avg_latency_ms`
- `last_score_at`

### Envelope

```go
type Envelope struct {
    Version     string
    MessageID   string
    TraceID     string
    SessionID   string
    OwnerAgent  string
    FromAgent   string
    ToAgent     string
    TaskClass   string
    Kind        string // task, reply, error
    TTL         int
    Prompt      string
    Metadata    map[string]any
}
```

### Candidate Reply

```go
type CandidateReply struct {
    AgentID             string
    Stage               string // "in_progress" | "final" | "error"
    Text                string
    Latency             time.Duration
    TokensUsed          int
    DeterministicScore  int
    JudgeScore          int
    PassedChecks        bool
    Err                 string
}
```

### Context Policy

```go
type ContextPolicy struct {
    MaxTokensHard int
    TrackStats    bool
}
```

### Timeout Policy

```go
type TimeoutPolicy struct {
    PeerTimeout  time.Duration
    OwnerTimeout time.Duration
    LogOnly      bool
}
```

### Memory Scope

```go
type MemoryScope struct {
    SessionID          string
    PrivateSemanticRO  []string
    SharedSemanticRW   []string
    CrossAgentReadOnly bool
}
```

---

## Cold Start Policy

- All agents start as `generalists`.
- No explicit capabilities are required at registration time.
- The first agent that receives a human request becomes `owner`.
- During cold start, owner uses `owner + sampled peers` evaluation instead of blind self-execution.
- Default sampling policy:
  - `sample_k = 2` peers when `online_agents >= 3`
  - `sample_k = 1` peer when `online_agents == 2`
  - `sample_k = 0` when no peers are online
- Routing after warmup:
  - prefer agents with strongest score for the task class
  - retain some exploration probability to avoid premature lock-in
- scoring and routing are always evaluated **within the current `task_class`**

---

## Task Classification

Initial coarse classes:
- `coding`
- `shell`
- `analysis`
- `research`
- `writing`

For MVP, owner chooses the class through a small LLM classification call before peer sampling. The classifier should return:
- one `task_class`
- optional confidence score
- short reasoning summary for logs only

The first slice should not use keyword heuristics as the primary classifier.

Default closed set:
- `coding`
- `shell`
- `analysis`
- `research`
- `writing`

---

## Winner Selection

Order of evaluation:
1. deterministic checks if available
2. explicit failure vs success
3. higher deterministic score
4. higher judge/owner score
5. lower latency as tie-breaker

Examples:
- code task: tests/build/lint
- shell task: command success and expected pattern
- writing task: owner-scored comparison

Judge stack for the first slice:
- `LLMJudge`
- fallback to `OwnerFallbackJudge`

For the first slice, it is enough to support:
- deterministic pass/fail
- `LLMJudge` when multiple valid candidates disagree
- owner fallback scoring when no deterministic check exists or judge output is invalid

---

## Error Handling

- `peer timeout`
  - candidate marked as non-final
  - owner proceeds with remaining candidates
- `invalid candidate reply`
  - candidate marked `Stage="error"`
  - scored below valid candidates
- `execution error`
  - candidate marked `Stage="error"`
  - owner may fallback to self or other peer
- `split decision`
  - routed to `LLMJudge`
  - owner fallback if judge fails

Principle for the first slice:
- do not fail the whole flow because one peer misbehaved
- degrade toward owner-local completion

---

### Task 1: Add Mesh Types And Registry Interface

**Files:**
- Create: `internal/mesh/types.go`
- Create: `internal/mesh/registry.go`

- [ ] **Step 1: Write the failing test for basic peer descriptor usage**

```go
func TestPeerDescriptorRoundTrip(t *testing.T) {
    peer := PeerDescriptor{AgentID: "agent-a", Model: "glm-5"}
    if peer.AgentID != "agent-a" {
        t.Fatalf("unexpected peer: %#v", peer)
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run TestPeerDescriptorRoundTrip -v`

Expected: FAIL because mesh types do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- `PeerDescriptor`
- `Envelope`
- `CandidateReply`
- registry interface methods:
  - `Register`
  - `Heartbeat`
  - `ListOnline`
  - `RecordScore`

- [ ] **Step 4: Run the test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run TestPeerDescriptorRoundTrip -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mesh/types.go internal/mesh/registry.go
git commit -m "feat: add mesh types and registry contracts"
```

### Task 2: Add Postgres Registry And Score Storage

**Files:**
- Create: `internal/mesh/postgres_registry.go`
- Create: `internal/mesh/postgres_registry_test.go`

- [ ] **Step 1: Write the failing tests for registration and score persistence**

```go
func TestPostgresRegistryRegistersAndListsOnlineAgents(t *testing.T) {}
func TestPostgresRegistryRecordsTaskScores(t *testing.T) {}
```

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run 'TestPostgresRegistryRegistersAndListsOnlineAgents|TestPostgresRegistryRecordsTaskScores' -v`

Expected: FAIL because Postgres registry does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- bootstrap schema for `mesh_agents` and `mesh_agent_scores`
- register/upsert peer rows
- heartbeat update
- list online peers by `last_seen_at`
- exclude stale peers using `TEAMD_MESH_STALE_THRESHOLD`
- persist/update score rows by `(agent_id, task_class)`
- keep peer `status` (`idle|busy|draining`) in registry rows

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run 'TestPostgresRegistryRegistersAndListsOnlineAgents|TestPostgresRegistryRecordsTaskScores' -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mesh/postgres_registry.go internal/mesh/postgres_registry_test.go
git commit -m "feat: add postgres mesh registry"
```

### Task 3: Add Cold Start Router

**Files:**
- Create: `internal/mesh/router.go`
- Create: `internal/mesh/router_test.go`

- [ ] **Step 1: Write the failing tests for cold-start sampling and warm routing**

```go
func TestRouterSamplesPeersDuringColdStart(t *testing.T) {}
func TestRouterPrefersHigherScoredPeersAfterWarmup(t *testing.T) {}
```

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run 'TestRouterSamplesPeersDuringColdStart|TestRouterPrefersHigherScoredPeersAfterWarmup' -v`

Expected: FAIL because router logic does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- owner retention rule
- `sample_k` cold-start policy
- warm preference based on task-class scores
- exploration probability to avoid permanent early lock-in
- per-peer timeout and partial-result fallback when sampled peers do not answer
- skip or down-rank non-`idle` peers

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run 'TestRouterSamplesPeersDuringColdStart|TestRouterPrefersHigherScoredPeersAfterWarmup' -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mesh/router.go internal/mesh/router_test.go
git commit -m "feat: add mesh cold start router"
```

### Task 4: Add LLM Task Classifier

**Files:**
- Create: `internal/mesh/classifier.go`
- Create: `internal/mesh/classifier_test.go`

- [ ] **Step 1: Write the failing test for classifier output parsing**

```go
func TestClassifierParsesTaskClassFromProviderReply(t *testing.T) {}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run TestClassifierParsesTaskClassFromProviderReply -v`

Expected: FAIL because classifier logic does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- classifier request contract
- provider-backed classification call
- strict parsing into one of:
  - `coding`
  - `shell`
  - `analysis`
  - `research`
  - `writing`
- fallback to `analysis` only when classifier output is invalid or empty

- [ ] **Step 4: Run the test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run TestClassifierParsesTaskClassFromProviderReply -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mesh/classifier.go internal/mesh/classifier_test.go
git commit -m "feat: add mesh llm task classifier"
```

### Task 5: Add Candidate Evaluator And Judge Stack

**Files:**
- Create: `internal/mesh/evaluator.go`
- Create: `internal/mesh/evaluator_test.go`

- [ ] **Step 1: Write the failing tests for winner selection**

```go
func TestEvaluatorPrefersPassingCandidate(t *testing.T) {}
func TestEvaluatorUsesLatencyAsTieBreaker(t *testing.T) {}
```

- [ ] **Step 2: Run focused tests to verify they fail**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run 'TestEvaluatorPrefersPassingCandidate|TestEvaluatorUsesLatencyAsTieBreaker' -v`

Expected: FAIL because evaluator logic does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- deterministic score comparison
- success/failure precedence
- `LLMJudge`
- `OwnerFallbackJudge`
- latency tie-breaker
- score update result structure

- [ ] **Step 4: Run focused tests to verify they pass**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run 'TestEvaluatorPrefersPassingCandidate|TestEvaluatorUsesLatencyAsTieBreaker' -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mesh/evaluator.go internal/mesh/evaluator_test.go
git commit -m "feat: add mesh candidate evaluator"
```

### Task 6: Add Peer Transport

**Files:**
- Create: `internal/mesh/http_transport.go`
- Create: `internal/mesh/http_transport_test.go`

- [ ] **Step 1: Write the failing test for envelope delivery**

```go
func TestHTTPTransportDeliversEnvelopeToPeer(t *testing.T) {}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run TestHTTPTransportDeliversEnvelopeToPeer -v`

Expected: FAIL because peer transport does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- `POST /mesh/message`
- request/response serialization
- reply envelope handling
- TTL decrement and dedupe guard hooks
- idempotency by `MessageID`
- owner-side dedupe by `TraceID`

Note:
- This first slice may use HTTP for speed.
- Keep envelope and handler boundaries transport-agnostic so they can later move to `gRPC` without changing router/evaluator logic.

- [ ] **Step 4: Run the test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh -run TestHTTPTransportDeliversEnvelopeToPeer -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mesh/http_transport.go internal/mesh/http_transport_test.go
git commit -m "feat: add mesh peer transport"
```

### Task 7: Wire Mesh Into Coordinator Startup

**Files:**
- Modify: `cmd/coordinator/main.go`
- Modify: `internal/config/config.go`

- [ ] **Step 1: Write the failing test for mesh config loading**

```go
func TestConfigLoadsMeshSettings(t *testing.T) {}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/config -run TestConfigLoadsMeshSettings -v`

Expected: FAIL because mesh config fields do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Add config:
- `TEAMD_AGENT_ID`
- `TEAMD_MESH_LISTEN_ADDR`
- `TEAMD_MESH_REGISTRY_DSN`
- `TEAMD_MESH_COLD_START_FANOUT`
- `TEAMD_MESH_EXPLORATION_RATE`
- `TEAMD_MESH_PEER_TIMEOUT`
- `TEAMD_MESH_HEARTBEAT_INTERVAL`
- `TEAMD_MESH_STALE_THRESHOLD`
- `TEAMD_MESH_OWNER_TIMEOUT`
- `TEAMD_MESH_TIMEOUT_LOG_ONLY`
- `TEAMD_MESH_CONTEXT_MAX_TOKENS_HARD`
- `TEAMD_MESH_CONTEXT_TRACK_STATS`

Wire coordinator startup:
- peer listener
- self-registration
- periodic heartbeat

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/config -run TestConfigLoadsMeshSettings -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add cmd/coordinator/main.go internal/config/config.go
git commit -m "feat: wire mesh startup into coordinator"
```

### Task 8: Make Telegram Ingress The Owner And Fan Out Candidates

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing test for owner-based candidate collection**

```go
func TestTelegramOwnerSamplesPeersAndChoosesWinner(t *testing.T) {}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestTelegramOwnerSamplesPeersAndChoosesWinner -v`

Expected: FAIL because Telegram ingress does not fan out to peers yet.

- [ ] **Step 3: Write minimal implementation**

Implement:
- owner trace creation from Telegram ingress
- task-class selection via LLM classifier
- peer sampling via router
- candidate collection from peers
- evaluator choice
- score persistence after winner selection
- trace idempotency so duplicate ingress does not produce duplicate evaluation
- `MemoryScope` wiring for session/private/shared memory access rules
- owner-side context statistics tracking

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/transport/telegram -run TestTelegramOwnerSamplesPeersAndChoosesWinner -v`

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add owner-based mesh evaluation for telegram ingress"
```

### Task 9: Document Cold Start And Soft Specialization

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update documentation**

Document:
- agents register as equal generalists
- owner is the ingress agent
- cold-start sampling policy
- soft specialization from scores, not fixed capabilities
- warm routing behavior
- `LLMTaskClassifier`
- `LLMJudge -> OwnerFallbackJudge`
- per-agent root workspace and cross-read through shared/artifact paths
- timeout and degradation policy

- [ ] **Step 2: Sanity-check docs**

Run: `rg -n "generalist|owner|cold-start|soft specialization|mesh" README.md`

Expected: matching lines exist and describe the model accurately.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: describe mesh cold start model"
```

### Task 10: Final Verification

**Files:**
- Modify: none

- [ ] **Step 1: Run mesh package tests**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mesh`

Expected: PASS

- [ ] **Step 2: Run full test suite**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`

Expected: PASS

- [ ] **Step 3: Manual cold start scenario**

Scenario:
1. start three equal agents
2. register all three
3. send one coding task to owner
4. owner samples peers
5. owner chooses winner
6. score rows update
7. send similar task again and verify warm routing is smarter

- [ ] **Step 4: Commit final verification if needed**

```bash
git add -A
git commit -m "test: verify mesh cold start flow" || true
```
