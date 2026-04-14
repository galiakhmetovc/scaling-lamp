# Go MCP Local Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Go-only MCP-style runtime layer that exposes local filesystem and shell tools to workers without introducing Node/npm/gateway dependencies.

**Architecture:** Keep MCP as an in-process Go runtime with registry, typed tool descriptors, and tool invocation contracts. The first concrete tool providers are local filesystem and shell adapters implemented in Go and registered through `internal/mcp`, so workers can list and call tools through one abstraction while the codebase remains aligned with the current Go-only architecture. The MCP runtime must be thread-safe, filesystem access must go through a configurable root plus path sanitization, and shell execution must have minimal non-negotiable guards such as timeout and no interactive mode. This slice intentionally does not implement the full deny-by-default policy baseline from spec; role allowlists, output limits, and stronger sandboxing stay in the separate `teamD-runtime-mcp-policy-baseline` task.

**Tech Stack:** Go 1.25+, standard library, current worker runtime, current MCP package, `os`, `filepath`, `os/exec`, `sync`, Go tests.

---

## File Structure

- Create: `internal/mcp/types.go`
  Purpose: define tool descriptor, tool input, tool result, and runtime contracts.
- Modify: `internal/mcp/runtime.go`
  Purpose: turn the current stub into a real registry and invocation runtime.
- Modify: `internal/config/config.go`
  Purpose: add MCP filesystem root configuration for local tool execution.
- Modify: `internal/mcp/registry.go`
  Purpose: keep helper logic aligned with typed tool descriptors.
- Create: `internal/mcp/tools/filesystem.go`
  Purpose: implement local filesystem tools (`read_file`, `write_file`, `list_dir`).
- Create: `internal/mcp/tools/shell.go`
  Purpose: implement local shell execution tool using `os/exec`.
- Create: `internal/mcp/runtime_test.go`
  Purpose: verify tool registration, listing, and invocation.
- Modify: `internal/worker/runtime.go`
  Purpose: hydrate tool descriptors and provide a call path for MCP tools.
- Modify: `tests/integration/coordinator_flow_test.go`
  Purpose: verify workers can see and call MCP tools.
- Modify: `README.md`
  Purpose: document local MCP tools, current access scope, and operational caveats.

### Task 1: Define MCP Tool Contracts

**Files:**
- Create: `internal/mcp/types.go`
- Modify: `internal/mcp/runtime.go`
- Modify: `internal/mcp/runtime_test.go`

- [ ] **Step 1: Write the failing test for tool registration and listing**

```go
func TestRuntimeListsRegisteredTools(t *testing.T) {
	runtime := mcp.NewRuntime()
	runtime.Register(mcp.Tool{
		Name:        "filesystem.read_file",
		Description: "Read a file",
	})

	tools, err := runtime.ListTools("researcher")
	if err != nil {
		t.Fatalf("list tools: %v", err)
	}
	if len(tools) != 1 || tools[0].Name != "filesystem.read_file" {
		t.Fatalf("unexpected tools: %#v", tools)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mcp -run TestRuntimeListsRegisteredTools -v`
Expected: FAIL because the MCP runtime does not yet have a real registry or tool contract.

- [ ] **Step 3: Write minimal implementation**

Create `internal/mcp/types.go` with:

```go
type Tool struct {
	Name        string
	Description string
	Call        func(context.Context, CallInput) (CallResult, error)
}

type CallInput struct {
	Arguments map[string]any
}

type CallResult struct {
	Content string
}
```

Update `internal/mcp/runtime.go` to expose:
- `NewRuntime()`
- `Register(tool Tool)`
- `ListTools(role string) ([]Tool, error)`
- `CallTool(ctx context.Context, name string, input CallInput) (CallResult, error)`

The runtime must use `sync.RWMutex` around its internal tool registry so concurrent worker access is safe.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mcp -run TestRuntimeListsRegisteredTools -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mcp/types.go internal/mcp/runtime.go internal/mcp/runtime_test.go
git commit -m "feat: add go mcp tool contracts"
```

### Task 2: Add Filesystem Tools

**Files:**
- Modify: `internal/config/config.go`
- Create: `internal/mcp/tools/filesystem.go`
- Modify: `internal/mcp/runtime_test.go`

- [ ] **Step 1: Write the failing test for reading and listing files**

```go
func TestFilesystemToolsReadAndList(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "note.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	runtime := mcp.NewRuntime()
	mcp.RegisterFilesystemTools(runtime)

	list, err := runtime.CallTool(context.Background(), "filesystem.list_dir", mcp.CallInput{
		Arguments: map[string]any{"path": dir},
	})
	if err != nil {
		t.Fatalf("list dir: %v", err)
	}
	if !strings.Contains(list.Content, "note.txt") {
		t.Fatalf("unexpected list result: %q", list.Content)
	}

	read, err := runtime.CallTool(context.Background(), "filesystem.read_file", mcp.CallInput{
		Arguments: map[string]any{"path": path},
	})
	if err != nil {
		t.Fatalf("read file: %v", err)
	}
	if read.Content != "hello" {
		t.Fatalf("unexpected file content: %q", read.Content)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mcp -run TestFilesystemToolsReadAndList -v`
Expected: FAIL because filesystem tools do not exist yet.

- [ ] **Step 3: Write minimal implementation**

Create `internal/mcp/tools/filesystem.go` with registration helpers for:
- `filesystem.read_file`
- `filesystem.write_file`
- `filesystem.list_dir`

Implementation notes:
- use `os.ReadFile`, `os.WriteFile`, `os.ReadDir`
- accept plain path arguments
- return simple text content for MVP
- add `sanitizePath(root, input)` and reject paths outside the configured root
- add `MCP_FS_ROOT` to config; for this slice, default may remain broad (for example `/`) if you want host-wide access, but the root must still be explicit and enforced
- keep path validation local to the tool layer even if the configured root is broad, so later hardening can tighten the same contract instead of refactoring call sites

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mcp ./internal/config -run TestFilesystemToolsReadAndList -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mcp/tools/filesystem.go internal/mcp/runtime_test.go
git commit -m "feat: add go filesystem mcp tools"
```

### Task 3: Add Shell Tool

**Files:**
- Create: `internal/mcp/tools/shell.go`
- Modify: `internal/mcp/runtime_test.go`

- [ ] **Step 1: Write the failing test for shell execution**

```go
func TestShellToolExecutesCommand(t *testing.T) {
	runtime := mcp.NewRuntime()
	mcp.RegisterShellTools(runtime)

	out, err := runtime.CallTool(context.Background(), "shell.exec", mcp.CallInput{
		Arguments: map[string]any{
			"command": "printf hello",
		},
	})
	if err != nil {
		t.Fatalf("shell exec: %v", err)
	}
	if out.Content != "hello" {
		t.Fatalf("unexpected shell output: %q", out.Content)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mcp -run TestShellToolExecutesCommand -v`
Expected: FAIL because shell tool does not exist yet.

- [ ] **Step 3: Write minimal implementation**

Create `internal/mcp/tools/shell.go`:
- register `shell.exec`
- use `exec.CommandContext("bash", "-lc", command)`
- support optional `cwd`
- enforce timeout from context
- capture combined stdout/stderr

For this slice, keep shell broad as requested, but still enforce:
- default timeout when the caller provides none
- reject obviously interactive invocations such as `-i` / `--interactive`
- log execution metadata rather than full potentially sensitive payloads
- prefer test commands that are stable on Linux, such as `printf hello`
- keep the result/error contract idiomatic Go: `CallResult, error`; do not add a second embedded error format in this slice

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./internal/mcp -run TestShellToolExecutesCommand -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/mcp/tools/shell.go internal/mcp/runtime_test.go
git commit -m "feat: add go shell mcp tool"
```

### Task 4: Wire MCP Runtime Into Worker

**Files:**
- Modify: `internal/worker/runtime.go`
- Modify: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing integration test for worker tool visibility**

```go
func TestWorkerHydratesLocalMCPTools(t *testing.T) {
	runtime := worker.NewRuntime(worker.TestDepsWithCapabilities(
		skills.StaticLoader{},
		mcp.NewRuntimeWithLocalTools(),
	))

	id, err := runtime.Start(context.Background(), worker.Spec{Role: "researcher"})
	if err != nil {
		t.Fatalf("start worker: %v", err)
	}

	snap := runtime.Snapshot(id)
	if len(snap.MCPServers) == 0 {
		t.Fatal("expected mcp tools in worker snapshot")
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestWorkerHydratesLocalMCPTools -v`
Expected: FAIL because the worker still only sees the old static MCP stub.

- [ ] **Step 3: Write minimal implementation**

Update `internal/worker/runtime.go` so worker dependencies can accept the real Go MCP runtime and expose registered tool names in snapshots.

If needed, add a helper:

```go
func NewRuntimeWithLocalTools() *Runtime
```

that pre-registers filesystem and shell tools.

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestWorkerHydratesLocalMCPTools -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/worker/runtime.go tests/integration/coordinator_flow_test.go
git commit -m "feat: expose local go mcp tools to workers"
```

### Task 5: Add End-to-End Tool Invocation Test

**Files:**
- Modify: `tests/integration/coordinator_flow_test.go`

- [ ] **Step 1: Write the failing integration test for actual tool call**

```go
func TestWorkerCallsFilesystemTool(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "note.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatalf("write fixture: %v", err)
	}

	runtime := worker.NewRuntime(worker.TestDepsWithCapabilities(
		skills.StaticLoader{},
		mcp.NewRuntimeWithLocalTools(),
	))

	out, err := runtime.CallTool(context.Background(), "filesystem.read_file", map[string]any{
		"path": path,
	})
	if err != nil {
		t.Fatalf("call tool: %v", err)
	}
	if out.Content != "hello" {
		t.Fatalf("unexpected tool output: %q", out.Content)
	}
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestWorkerCallsFilesystemTool -v`
Expected: FAIL because worker runtime does not yet expose a call path.

- [ ] **Step 3: Write minimal implementation**

Add a worker method that forwards tool calls into the MCP runtime:

```go
func (r *Runtime) CallTool(ctx context.Context, name string, args map[string]any) (mcp.CallResult, error)
```

- [ ] **Step 4: Run test to verify it passes**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./tests/integration -run TestWorkerCallsFilesystemTool -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/worker/runtime.go tests/integration/coordinator_flow_test.go
git commit -m "feat: add worker mcp tool invocation path"
```

### Task 6: Document Local MCP Runtime

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Write the documentation gap check**

Run: `rg -n "MCP|filesystem|shell.exec|local tools" README.md`
Expected: missing or incomplete coverage

- [ ] **Step 2: Write minimal documentation**

Document:
- that MCP is implemented in-process in Go for MVP
- available local tools
- that filesystem access is enforced under `MCP_FS_ROOT`
- that shell currently has broad access but is not sandboxed
- that hardening is a follow-up step
- that role-based allowlists, output caps, and policy enforcement are tracked separately in `teamD-runtime-mcp-policy-baseline`

- [ ] **Step 3: Verify docs are present**

Run: `rg -n "MCP|filesystem|shell.exec|local tools|broad access|MCP_FS_ROOT|not sandboxed" README.md`
Expected: matching lines

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: describe go mcp local tools runtime"
```

### Task 7: Final Verification

**Files:**
- Verify only

- [ ] **Step 1: Run full test suite**

Run: `mkdir -p .tmp/go && GOTMPDIR=$PWD/.tmp/go go test ./...`
Expected: PASS

- [ ] **Step 2: Run live runtime manually**

Run:

```bash
mkdir -p .tmp/go
set -a && . ./.env && set +a
GOTMPDIR=$PWD/.tmp/go go run ./cmd/coordinator
```

Expected:
- worker can list local MCP tools
- filesystem tool can read files
- shell tool can execute a simple command

- [ ] **Step 3: Commit final verified changes**

```bash
git add internal/mcp/types.go internal/mcp/runtime.go internal/mcp/registry.go internal/mcp/tools/filesystem.go internal/mcp/tools/shell.go internal/mcp/runtime_test.go internal/worker/runtime.go tests/integration/coordinator_flow_test.go README.md
git commit -m "feat: add go mcp runtime with local tools"
```
