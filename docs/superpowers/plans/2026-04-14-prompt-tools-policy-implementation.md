# Prompt And Tool Policy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add clean-room `PromptAssemblyContract`, `ToolContract`, and `ToolExecutionContract` so system prompt, session head, tool exposure, and tool execution safety are resolved from policy-driven config instead of implicit runtime behavior.

**Architecture:** Extend the existing contract-path plus policy-strategy architecture rather than adding special cases to `chat`, `provider`, or `request-shape`. `PromptAssemblyExecutor` becomes the source of top-of-prompt messages; `ToolContract` becomes the source of model-visible tools; `ToolExecutionContract` becomes the gate for tool-call execution. `RequestShapeExecutor` and `ProviderClient` consume already-resolved prompt and tool surfaces instead of assembling them ad hoc.

**Tech Stack:** Go, YAML config graph, `internal/contracts`, `internal/config`, `internal/policies`, `internal/runtime`, `internal/provider`, stdlib tests.

---

## File Structure And Responsibility

### Existing Files To Modify

- Modify: `internal/contracts/contracts.go`
  Responsibility: add resolved contract and policy structs for prompt assembly, tools, and tool execution.
- Modify: `internal/policies/registry.go`
  Responsibility: register new policy families, kinds, and strategy names.
- Modify: `internal/config/registry.go`
  Responsibility: register new contract kinds and policy module kinds.
- Modify: `internal/runtime/contract_resolver.go`
  Responsibility: decode and resolve new contracts from YAML modules into runtime structs.
- Modify: `internal/runtime/agent_builder.go`
  Responsibility: build new executors and wire them into runtime agent surface.
- Modify: `internal/provider/request_shape_executor.go`
  Responsibility: consume resolved prompt assembly output and resolved tool surface instead of raw ad hoc inputs.
- Modify: `internal/provider/client.go`
  Responsibility: accept assembled prompt and resolved visible tools, then later route tool calls into execution gate.
- Modify: `internal/runtime/chat.go`
  Responsibility: chat turns should use prompt assembly and visible tools through resolved contracts.
- Modify: `internal/runtime/smoke.go`
  Responsibility: smoke path should use the same prompt/tool pipeline as chat.
- Modify: `internal/runtime/projections/transcript.go`
  Responsibility: ensure prompt assembly can reliably read ordered transcript data.

### New Files To Create

- Create: `internal/promptassembly/executor.go`
  Responsibility: build top-of-prompt messages from system prompt file and session-head projection.
- Create: `internal/promptassembly/executor_test.go`
  Responsibility: TDD coverage for file-backed system prompt and `messages[0]` session head behavior.
- Create: `internal/tools/catalog.go`
  Responsibility: resolve visible tool definitions from `ToolContract`.
- Create: `internal/tools/catalog_test.go`
  Responsibility: TDD coverage for allowlist selection and serialization surface.
- Create: `internal/tools/execution_gate.go`
  Responsibility: evaluate access, approval, and sandbox policies before execution.
- Create: `internal/tools/execution_gate_test.go`
  Responsibility: TDD coverage for allow/deny/approval/sandbox decision logic.
- Create: `config/zai-smoke/contracts/prompt-assembly.yaml`
- Create: `config/zai-smoke/contracts/tools.yaml`
- Create: `config/zai-smoke/contracts/tool-execution.yaml`
- Create: `config/zai-smoke/policies/prompt-assembly/system-prompt.yaml`
- Create: `config/zai-smoke/policies/prompt-assembly/session-head.yaml`
- Create: `config/zai-smoke/policies/tools/catalog.yaml`
- Create: `config/zai-smoke/policies/tools/serialization.yaml`
- Create: `config/zai-smoke/policies/tool-execution/access.yaml`
- Create: `config/zai-smoke/policies/tool-execution/approval.yaml`
- Create: `config/zai-smoke/policies/tool-execution/sandbox.yaml`
- Create: `config/zai-smoke/prompts/system.md`
  Responsibility: shipped file-backed system prompt for the baseline config graph.

### Docs To Modify

- Modify: `docs/clean-room-current-policies-and-strategies.md`
- Modify: `docs/clean-room-current-runtime-flow.md`
- Modify: `docs/clean-room-current-system-detailed.md`
- Modify: `README.md`

## Task 1: Add contract and registry types for new policy domains

**Files:**
- Modify: `internal/contracts/contracts.go`
- Modify: `internal/policies/registry.go`
- Modify: `internal/config/registry.go`
- Test: `internal/policies/registry_test.go`

- [ ] **Step 1: Write failing registry tests for the new families and strategies**

Add tests covering:
- `system-prompt` family with `file_static`
- `session-head` family with `off`, `projection_summary`
- `tool-catalog` family with `static_allowlist`
- `tool-serialization` family with `openai_function_tools`
- `tool-access` family with `static_allowlist`, `deny_all`
- `tool-approval` family with `always_allow`, `always_require`, `require_for_destructive`
- `tool-sandbox` family with `default_runtime`, `read_only`, `workspace_write`, `deny_exec`

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/policies -run 'TestBuiltInRegistry|TestRegistry' -count=1`
Expected: FAIL because the new families and strategies are not registered yet.

- [ ] **Step 3: Add minimal contract structs and policy params**

Add new resolved contract fields and structs:
- `ResolvedContracts.PromptAssembly`
- `ResolvedContracts.Tools`
- `ResolvedContracts.ToolExecution`
- `PromptAssemblyContract`
- `SystemPromptPolicy`
- `SessionHeadPolicy`
- `ToolContract`
- `ToolCatalogPolicy`
- `ToolSerializationPolicy`
- `ToolExecutionContract`
- `ToolAccessPolicy`
- `ToolApprovalPolicy`
- `ToolSandboxPolicy`

Keep params minimal and aligned with the spec:
- `SystemPromptParams { Path, Role, Required, TrimTrailingWhitespace }`
- `SessionHeadParams { Placement, Title, MaxItems, IncludeSessionID, IncludeOpenLoops, IncludeLastUserMessage, IncludeLastAssistantMessage }`
- `ToolCatalogParams { ToolIDs, AllowEmpty, Dedupe }`
- `ToolSerializationParams { StrictJSONSchema, IncludeDescriptions }`
- `ToolAccessParams { ToolIDs }`
- `ToolApprovalParams { DestructiveToolIDs, ApprovalMessageTemplate }`
- `ToolSandboxParams { AllowNetwork, AllowWritePaths, DenyWritePaths, Timeout, MaxOutputBytes }`

- [ ] **Step 4: Register new policy families, strategy names, and config kinds**

Update:
- `internal/policies/registry.go`
- `internal/config/registry.go`

- [ ] **Step 5: Run tests to verify they pass**

Run: `go test ./internal/policies ./internal/config -count=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/contracts/contracts.go internal/policies/registry.go internal/config/registry.go internal/policies/registry_test.go
git commit -m "feat: add prompt and tool policy families"
```

## Task 2: Resolve PromptAssemblyContract from config modules

**Files:**
- Modify: `internal/runtime/contract_resolver.go`
- Test: `internal/runtime/contract_resolver_test.go`
- Create: `config/zai-smoke/contracts/prompt-assembly.yaml`
- Create: `config/zai-smoke/policies/prompt-assembly/system-prompt.yaml`
- Create: `config/zai-smoke/policies/prompt-assembly/session-head.yaml`
- Create: `config/zai-smoke/prompts/system.md`
- Modify: `config/zai-smoke/agent.yaml`

- [ ] **Step 1: Write failing resolver tests for PromptAssemblyContract**

Add tests for:
- loading `PromptAssemblyContractConfig`
- decoding `SystemPromptPolicy.file_static`
- decoding `SessionHeadPolicy.projection_summary`
- rejecting missing required policy paths

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/runtime -run 'TestResolveContracts.*PromptAssembly' -count=1`
Expected: FAIL because the resolver does not know the new contract.

- [ ] **Step 3: Implement contract body decoding and resolution**

Add:
- `promptAssemblyContractBody`
- `resolvePromptAssemblyContract(...)`

Populate:
- `ResolvedContracts.PromptAssembly`

- [ ] **Step 4: Add baseline shipped config**

Create YAML modules and `prompts/system.md` with a simple, explicit baseline prompt.

- [ ] **Step 5: Run tests to verify they pass**

Run: `go test ./internal/runtime ./internal/config -count=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime/contract_resolver.go internal/runtime/contract_resolver_test.go config/zai-smoke/contracts/prompt-assembly.yaml config/zai-smoke/policies/prompt-assembly/system-prompt.yaml config/zai-smoke/policies/prompt-assembly/session-head.yaml config/zai-smoke/prompts/system.md config/zai-smoke/agent.yaml
git commit -m "feat: add prompt assembly contract resolution"
```

## Task 3: Implement PromptAssemblyExecutor with `messages[0]` session head

**Files:**
- Create: `internal/promptassembly/executor.go`
- Create: `internal/promptassembly/executor_test.go`
- Modify: `internal/runtime/agent_builder.go`
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/smoke.go`
- Modify: `internal/runtime/projections/transcript.go` if ordering gaps appear

- [ ] **Step 1: Write failing executor tests**

Cover:
- system prompt loaded from file as a separate prompt layer
- session head rendered from projections
- `placement=message0` puts session head at `messages[0]`
- transcript follows after prompt assembly prefix
- missing required prompt file fails clearly

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/promptassembly -count=1`
Expected: FAIL because executor does not exist.

- [ ] **Step 3: Implement minimal PromptAssemblyExecutor**

Responsibilities:
- load file-backed system prompt
- normalize text
- read transcript/session projection inputs
- build session head summary
- enforce `messages[0]` for session head baseline
- return assembled prompt messages for downstream request shape

- [ ] **Step 4: Wire prompt assembly into builder and runtime**

`AgentBuilder`, `ChatTurn`, and `Smoke` must stop hand-assembling top-of-prompt behavior and instead use the executor.

- [ ] **Step 5: Run targeted tests**

Run: `go test ./internal/promptassembly ./internal/runtime ./cmd/agent -count=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/promptassembly/executor.go internal/promptassembly/executor_test.go internal/runtime/agent_builder.go internal/runtime/chat.go internal/runtime/smoke.go internal/runtime/projections/transcript.go
git commit -m "feat: add prompt assembly executor"
```

## Task 4: Resolve ToolContract and add visible-tool catalog

**Files:**
- Modify: `internal/runtime/contract_resolver.go`
- Test: `internal/runtime/contract_resolver_test.go`
- Create: `internal/tools/catalog.go`
- Create: `internal/tools/catalog_test.go`
- Create: `config/zai-smoke/contracts/tools.yaml`
- Create: `config/zai-smoke/policies/tools/catalog.yaml`
- Create: `config/zai-smoke/policies/tools/serialization.yaml`
- Modify: `config/zai-smoke/agent.yaml`

- [ ] **Step 1: Write failing tests for tool contract resolution and catalog selection**

Cover:
- contract loads both catalog and serialization policy
- `static_allowlist` preserves configured tool order
- unknown tool ids fail
- empty selection obeys `allow_empty`

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/runtime ./internal/tools -run 'TestResolveContracts.*Tool|TestToolCatalog' -count=1`
Expected: FAIL

- [ ] **Step 3: Implement ToolContract resolution**

Add:
- `toolContractBody`
- `resolveToolContract(...)`

- [ ] **Step 4: Implement minimal tool catalog executor**

Use a static allowlist over the registered tool surface already available in runtime.

- [ ] **Step 5: Run tests to verify they pass**

Run: `go test ./internal/runtime ./internal/tools -count=1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/runtime/contract_resolver.go internal/runtime/contract_resolver_test.go internal/tools/catalog.go internal/tools/catalog_test.go config/zai-smoke/contracts/tools.yaml config/zai-smoke/policies/tools/catalog.yaml config/zai-smoke/policies/tools/serialization.yaml config/zai-smoke/agent.yaml
git commit -m "feat: add tool contract and catalog"
```

## Task 5: Make RequestShapeExecutor consume resolved tool surface

**Files:**
- Modify: `internal/provider/request_shape_executor.go`
- Modify: `internal/provider/request_shape_executor_test.go`
- Modify: `internal/provider/client.go`
- Modify: `internal/runtime/chat.go`
- Modify: `internal/runtime/smoke.go`

- [ ] **Step 1: Write failing tests for request-shape integration**

Cover:
- request body uses visible tools from `ToolContract`
- `openai_function_tools` serializes expected provider format
- no raw ad hoc inline tool list remains the source of truth

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/provider -run 'TestRequestShape|TestProviderClient' -count=1`
Expected: FAIL

- [ ] **Step 3: Implement minimal integration**

Refactor:
- request-shape takes already selected visible tools
- serialization policy controls emitted provider `tools`
- chat/smoke path passes visible tools from catalog executor

- [ ] **Step 4: Run tests to verify they pass**

Run: `go test ./internal/provider ./internal/runtime -count=1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/provider/request_shape_executor.go internal/provider/request_shape_executor_test.go internal/provider/client.go internal/runtime/chat.go internal/runtime/smoke.go
git commit -m "feat: route visible tools through request shape"
```

## Task 6: Resolve ToolExecutionContract and build execution gate

**Files:**
- Modify: `internal/runtime/contract_resolver.go`
- Test: `internal/runtime/contract_resolver_test.go`
- Create: `internal/tools/execution_gate.go`
- Create: `internal/tools/execution_gate_test.go`
- Create: `config/zai-smoke/contracts/tool-execution.yaml`
- Create: `config/zai-smoke/policies/tool-execution/access.yaml`
- Create: `config/zai-smoke/policies/tool-execution/approval.yaml`
- Create: `config/zai-smoke/policies/tool-execution/sandbox.yaml`
- Modify: `internal/provider/client.go` if tool-call handling needs explicit gate hook
- Modify: `internal/runtime/chat.go`

- [ ] **Step 1: Write failing tests for execution gate decisions**

Cover:
- `static_allowlist` allows configured tools and rejects others
- `deny_all` rejects everything
- `always_allow` returns no approval requirement
- `default_runtime` returns baseline runtime restrictions

- [ ] **Step 2: Run tests to verify they fail**

Run: `go test ./internal/tools -run 'TestExecutionGate' -count=1`
Expected: FAIL

- [ ] **Step 3: Implement ToolExecutionContract resolution**

Add:
- `toolExecutionContractBody`
- `resolveToolExecutionContract(...)`

- [ ] **Step 4: Implement minimal execution gate**

The first slice only needs to produce a structured decision object:
- allowed or denied
- approval required or not
- sandbox settings selected

Do not add full approval UX in this task.

- [ ] **Step 5: Integrate gate into tool-call execution path**

Before any tool call is executed:
- resolve gate decision
- reject denied tools with explicit runtime error
- use returned sandbox settings for execution path

- [ ] **Step 6: Run tests to verify they pass**

Run: `go test ./internal/tools ./internal/runtime ./internal/provider -count=1`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add internal/runtime/contract_resolver.go internal/runtime/contract_resolver_test.go internal/tools/execution_gate.go internal/tools/execution_gate_test.go config/zai-smoke/contracts/tool-execution.yaml config/zai-smoke/policies/tool-execution/access.yaml config/zai-smoke/policies/tool-execution/approval.yaml config/zai-smoke/policies/tool-execution/sandbox.yaml internal/runtime/chat.go internal/provider/client.go
git commit -m "feat: add tool execution contract and gate"
```

## Task 7: Update documentation and shipped runtime references

**Files:**
- Modify: `docs/clean-room-current-policies-and-strategies.md`
- Modify: `docs/clean-room-current-runtime-flow.md`
- Modify: `docs/clean-room-current-system-detailed.md`
- Modify: `README.md`

- [ ] **Step 1: Write doc updates reflecting the new domains**

Document:
- prompt assembly order
- system prompt file path
- `messages[0]` session head rule
- tool visibility vs execution safety boundary
- shipped `zai-smoke` config choices

- [ ] **Step 2: Run one final verification pass**

Run:
```bash
go test ./cmd/agent ./internal/config ./internal/contracts ./internal/policies ./internal/provider ./internal/runtime ./internal/runtime/projections ./internal/promptassembly ./internal/tools -count=1
go build ./cmd/agent
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add docs/clean-room-current-policies-and-strategies.md docs/clean-room-current-runtime-flow.md docs/clean-room-current-system-detailed.md README.md
git commit -m "docs: describe prompt and tool policy domains"
```

## Recommended Execution Order

1. `teamD-1oy.3` — PromptAssemblyContract
2. `teamD-1oy.1` — ToolContract
3. `teamD-1oy.2` — ToolExecutionContract

That order is intentional:

- prompt assembly is independent and unblocks system prompt plus session head
- tool exposure must be resolved before request-shape can stop owning tool selection
- tool execution safety comes last because it depends on a clean model-visible tool identity surface

## Mandatory Smell Checks During Implementation

Stop and refactor if any of these happen:

- `chat.go` starts reading prompt files directly
- `request_shape_executor.go` starts deciding which tools exist
- tool allow/deny logic appears in `client.go` or `chat.go` instead of an execution gate
- system prompt text and session head text get collapsed into one generic blob
- the same tool selection rule exists in both contract resolution and request-shape execution

## Session Completion Checklist

- [ ] Close completed beads tasks
- [ ] Run full verification suite
- [ ] `git pull --rebase origin rewrite/clean-room-root`
- [ ] `bd dolt push` if available
- [ ] `git push origin rewrite/clean-room-root`
- [ ] confirm `git status` shows branch synced with origin
