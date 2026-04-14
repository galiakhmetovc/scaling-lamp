# Runtime Governance Policy Design

## Goal

Add one runtime-owned policy layer that becomes the canonical source of truth for:

- effective runtime policy resolution per session
- MCP tool allow/deny decisions
- approval requirements
- execution limits for risky local tools

## Why

`teamD` already has several policy-shaped pieces:

- `ActionPolicy`
- `MemoryPolicy`
- session overrides
- approval-required tools
- MCP tool runtime

They work, but they are spread across runtime, transport, and tool registration code.

The immediate problem is not “missing security” in the abstract.  
The immediate problem is that live tool execution does not have one runtime-owned policy decision point.

## Scope

This slice intentionally does not build a full policy engine.

It adds:

1. `PolicyResolver`
2. MCP execution policy types
3. runtime-owned tool execution decision API
4. baseline enforcement for local MCP tools

It does not add:

- hot-reloaded `policy.yaml`
- remote authz
- distributed policy sync
- full replay mode

## Design

### 1. PolicyResolver

Add a runtime-owned resolver that takes:

- runtime defaults
- base `MemoryPolicy`
- base `ActionPolicy`
- session overrides

and returns one explicit effective bundle.

Candidate shape:

```go
type EffectivePolicy struct {
    Summary RuntimeSummary
    MCP     MCPPolicy
}
```

The important part is not the struct name.  
The important part is to stop recomputing effective policy ad hoc in API and transport helpers.

### 2. MCPPolicy

Add one baseline policy contract for local tool execution:

```go
type MCPPolicy struct {
    Mode             string
    AllowedTools     []string
    RequireApproval  []string
    ShellTimeout     time.Duration
    MaxOutputBytes   int
    MaxOutputLines   int
}
```

Baseline behavior:

- deny by default for MCP tools not in allowlist
- explicit allowlist for the currently supported local tools
- explicit approval requirement for risky tools
- shell timeout and output limits as policy values, not hardcoded constants

### 3. Execution Decision API

Add one runtime-owned decision path:

```go
type ToolExecutionDecision struct {
    Allowed          bool
    RequiresApproval bool
    Reason           string
    Policy           MCPToolPolicy
}
```

Transports and tool execution code should ask runtime policy:

- is this tool allowed?
- does it need approval?
- what execution limits apply?

instead of inferring from scattered config.

### 4. MCP Enforcement

Enforce the policy in two places:

- tool listing path
- tool execution path

This means:

- tools not allowed by effective policy are not exposed to the provider
- direct execution also re-checks policy defensively

### 5. Relationship To Existing Policies

`ActionPolicy` remains the place for approval-required actions for now.

`MCPPolicy` is the execution surface for local tools.

The resolver composes them into one runtime-owned answer so API, Telegram, and future surfaces do not each rebuild effective policy differently.

## Success Criteria

- one runtime package answers effective policy questions
- local MCP tool exposure is allowlist-driven
- shell timeout is policy-driven, not hardcoded only in tool code
- oversized output is trimmed by policy-backed limits
- Telegram transport stops deciding live tool policy by itself

## Non-goals

- no auth boundary changes
- no provider sandbox rewrite
- no mesh policy integration in this slice
