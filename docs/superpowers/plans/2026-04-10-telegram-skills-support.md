# Telegram Skills Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add file-based `SKILL.md` support to the single-agent Telegram bot so skills can live in the workspace, be inspected and controlled from Telegram, and be injected into the runtime prompt in a predictable way.

**Architecture:** Replace the current `skills.StaticLoader` placeholder with a filesystem-backed skill runtime. Skills are discovered from workspace folders, parsed from `SKILL.md`, exposed through Telegram commands and model-facing inspection tools, and composed into the direct `1-1` runtime prompt after `AGENTS.md`. MVP is single-agent only; mesh-aware skill routing stays out of scope.

**Tech Stack:** Go, existing `internal/skills`, Telegram adapter, workspace-root discovery, file-based `SKILL.md` bundles.

---

## Scope

- Single-agent Telegram runtime only
- Workspace-local skills loaded from disk
- `SKILL.md` as the source of truth
- Manual visibility and activation from Telegram
- Compact `Available skills` catalog in prompt
- Model-facing `skills.list` and `skills.read`
- Prompt injection for active skills only

## Non-Goals

- Mesh-wide skill orchestration
- Auto-install from remote registries
- Full Codex/OpenClaw reference chasing and script execution semantics
- Arbitrary skill toolchains beyond prompt-layer support
- Automatic skill selection without user visibility

## Working Assumptions

- Workspace root is already explicit via `TEAMD_WORKSPACE_ROOT`
- Only `AGENTS.md` is auto-injected today
- Current `internal/skills` is intentionally minimal and can be evolved without large compatibility costs
- Local OpenClaw source for skills runtime is not available in this environment; plan targets the behavior shape the user wants: folder-based `SKILL.md` skills managed by the bot

## Target UX

Slash commands:

- `/skills`
  - show current active skills and workspace path
- `/skills list`
  - show discovered skills with name + short description
- `/skills show <name>`
  - show skill metadata and a shortened preview of `SKILL.md`
- `/skills use <name>`
  - activate a skill for the current session
- `/skills drop <name>`
  - deactivate a skill for the current session
- `/skills reset`
  - clear all active skills for the current session

Model-facing tools:

- `skills.list`
  - returns compact `name + description` summaries
- `skills.read(name)`
  - returns structured details for one skill
  - does not activate the skill

Prompt layering in direct mode:

1. base system prompt
2. `AGENTS.md`
3. compact `Available skills` catalog
4. active skill prompts in deterministic order
5. conversation history and user message

## File Map

- Create: `internal/skills/filesystem.go`
- Create: `internal/skills/filesystem_test.go`
- Create: `internal/skills/metadata.go`
- Create: `internal/skills/metadata_test.go`
- Create: `internal/skills/session.go`
- Create: `internal/skills/session_test.go`
- Create: `internal/skills/tools.go`
- Create: `internal/skills/tools_test.go`
- Modify: `internal/skills/runtime.go`
- Modify: `internal/skills/prompts.go`
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`
- Modify: `internal/config/config.go`
- Modify: `internal/config/config_test.go`
- Modify: `cmd/coordinator/main.go`
- Create: `skills/example/SKILL.md`
- Modify: `skills/README.md`

## Data Model

```go
type Bundle struct {
    Name        string
    Description string
    Path        string
    Prompt      string
}

type Catalog interface {
    List() ([]Bundle, error)
    Get(name string) (Bundle, bool, error)
}

type Summary struct {
    Name        string
    Description string
}

type Detail struct {
    Name        string
    Description string
    Prompt      string
    Version     string
}

type SessionState interface {
    Active(sessionKey string) []string
    Activate(sessionKey, skill string)
    Deactivate(sessionKey, skill string)
    Reset(sessionKey)
}
```

## Skill Discovery Rules

- Search under `<workspace_root>/skills`
- One skill = one directory containing `SKILL.md`
- Directory name is fallback skill name
- Parse optional frontmatter-like header when present:
  - `name`
  - `description`
- If header is absent:
  - `Name` = directory name
  - `Description` = first non-empty paragraph or empty string
- Ignore invalid directories without `SKILL.md`
- Discovery is strictly one level deep in MVP: `<workspace_root>/skills/*/SKILL.md`

## Prompt Composition Rules

- Skill prompt is the body of `SKILL.md`, not the raw full file with extra transport noise
- Model always sees a compact `Available skills` catalog with one short line per skill
- Preserve skill text verbatim except for metadata/header stripping
- Active skills are injected in stable lexical order by skill name
- Apply a total character budget for combined skills to avoid prompt blow-up
- If a skill exceeds the budget alone:
  - include a truncated preview with explicit marker

## Telegram Behavior Rules

- `/skills` commands are reserved commands like `/status`
- Skill activation is session-scoped, not global
- Direct `1-1` mode reads session skills on every run
- If a user activates an unknown skill:
  - respond with a concise error and suggest `/skills list`
- `skills.read` is inspection-only and never changes active state

## Acceptance Criteria

- Bot can discover local `SKILL.md` skills from workspace
- User can inspect and toggle them from Telegram
- Model can inspect skill catalog and details without forcing activation
- Activated skills actually appear in provider prompt requests
- Session A and Session B can have different active skill sets
- Invalid skill folders do not crash the bot
- Prompt size remains bounded with multiple skills

### Task 1: Add `SKILL.md` Metadata Parsing

**Files:**
- Create: `internal/skills/metadata.go`
- Test: `internal/skills/metadata_test.go`

- [ ] **Step 1: Write the failing test for parsing a `SKILL.md` file**

```go
func TestParseSkillMarkdownExtractsNameDescriptionAndPrompt(t *testing.T) {
    raw := "---\nname: deploy\ndescription: Safe deploy workflow\n---\n\n# Deploy\n\nUse this workflow."
    bundle, err := ParseMarkdown("skills/deploy/SKILL.md", raw)
    if err != nil {
        t.Fatal(err)
    }
    if bundle.Name != "deploy" || bundle.Description != "Safe deploy workflow" {
        t.Fatalf("unexpected metadata: %#v", bundle)
    }
    if !strings.Contains(bundle.Prompt, "Use this workflow.") {
        t.Fatalf("expected prompt body, got %q", bundle.Prompt)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/skills -run TestParseSkillMarkdownExtractsNameDescriptionAndPrompt -v`
Expected: FAIL because parser does not exist yet

- [ ] **Step 3: Implement minimal parser**

Implement frontmatter-aware parsing with fallback behavior.

- [ ] **Step 4: Add fallback-name test**

```go
func TestParseSkillMarkdownFallsBackToDirectoryName(t *testing.T) { /* ... */ }
```

- [ ] **Step 5: Run focused tests**

Run: `go test ./internal/skills -run 'TestParseSkillMarkdown' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/skills/metadata.go internal/skills/metadata_test.go
git commit -m "feat: parse skill markdown metadata"
```

### Task 2: Add Filesystem Skill Catalog

**Files:**
- Create: `internal/skills/filesystem.go`
- Test: `internal/skills/filesystem_test.go`
- Modify: `internal/skills/runtime.go`

- [ ] **Step 1: Write the failing discovery test**

```go
func TestFilesystemCatalogListsWorkspaceSkills(t *testing.T) {
    root := t.TempDir()
    os.MkdirAll(filepath.Join(root, "skills", "deploy"), 0o755)
    os.WriteFile(filepath.Join(root, "skills", "deploy", "SKILL.md"), []byte("# Deploy"), 0o644)

    catalog := NewFilesystemCatalog(root)
    bundles, err := catalog.List()
    if err != nil {
        t.Fatal(err)
    }
    if len(bundles) != 1 || bundles[0].Name != "deploy" {
        t.Fatalf("unexpected bundles: %#v", bundles)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/skills -run TestFilesystemCatalogListsWorkspaceSkills -v`
Expected: FAIL

- [ ] **Step 3: Implement workspace discovery**

Rules:
- only `<root>/skills/*/SKILL.md`
- deterministic lexical ordering
- invalid folders skipped

- [ ] **Step 4: Add invalid-folder and get-by-name tests**

- [ ] **Step 5: Run focused tests**

Run: `go test ./internal/skills -run 'TestFilesystemCatalog' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/skills/runtime.go internal/skills/filesystem.go internal/skills/filesystem_test.go
git commit -m "feat: add filesystem skill catalog"
```

### Task 3: Add Session-Scoped Skill State

**Files:**
- Create: `internal/skills/session.go`
- Test: `internal/skills/session_test.go`

- [ ] **Step 1: Write the failing session-state test**

```go
func TestSessionStateTracksActiveSkillsPerSession(t *testing.T) {
    state := NewSessionState()
    state.Activate("chat:1/default", "deploy")
    state.Activate("chat:1/default", "shell")
    state.Activate("chat:1/ops", "incident")

    got := state.Active("chat:1/default")
    if diff := cmp.Diff([]string{"deploy", "shell"}, got); diff != "" {
        t.Fatal(diff)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/skills -run TestSessionStateTracksActiveSkillsPerSession -v`
Expected: FAIL

- [ ] **Step 3: Implement in-memory session skill state**

- [ ] **Step 4: Add tests for deactivate/reset/idempotency**

- [ ] **Step 5: Run focused tests**

Run: `go test ./internal/skills -run 'TestSessionState' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/skills/session.go internal/skills/session_test.go
git commit -m "feat: add session skill state"
```

### Task 4: Add Prompt Composition With Skill Budgeting

**Files:**
- Modify: `internal/skills/prompts.go`
- Test: `internal/skills/filesystem_test.go`

- [ ] **Step 1: Write the failing prompt-composition test**

```go
func TestComposePromptOrdersAndBoundsSkillPrompts(t *testing.T) {
    bundles := []Bundle{
        {Name: "b", Prompt: "second"},
        {Name: "a", Prompt: "first"},
    }
    out := ComposePrompt(bundles)
    if !strings.Contains(out, "first") || !strings.Contains(out, "second") {
        t.Fatalf("missing prompts: %q", out)
    }
    if strings.Index(out, "first") > strings.Index(out, "second") {
        t.Fatalf("expected lexical order: %q", out)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `go test ./internal/skills -run TestComposePromptOrdersAndBoundsSkillPrompts -v`
Expected: FAIL

- [ ] **Step 3: Implement stable composition and total budget**

- [ ] **Step 4: Add truncation test**

- [ ] **Step 4.5: Add available-skills catalog test**

```go
func TestComposeCatalogBuildsCompactAvailableSkillsSection(t *testing.T) {
    bundles := []Bundle{
        {Name: "deploy", Description: "Safe deploy workflow"},
        {Name: "incident", Description: "Incident triage"},
    }
    out := ComposeCatalog(bundles)
    if !strings.Contains(out, "deploy") || !strings.Contains(out, "incident") {
        t.Fatalf("missing skills: %q", out)
    }
}
```

- [ ] **Step 5: Run focused tests**

Run: `go test ./internal/skills -run 'TestComposePrompt|TestComposeCatalog' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/skills/prompts.go internal/skills/filesystem_test.go
git commit -m "feat: bound and order skill prompt composition"
```

### Task 5: Add Skills Inspection Tools

**Files:**
- Create: `internal/skills/tools.go`
- Test: `internal/skills/tools_test.go`

- [ ] **Step 1: Write failing tests for `skills.list` and `skills.read`**

```go
func TestToolListReturnsCompactSummaries(t *testing.T) { /* ... */ }
func TestToolReadReturnsOneSkillDetail(t *testing.T) { /* ... */ }
```

- [ ] **Step 2: Run focused tests**

Run: `go test ./internal/skills -run 'TestToolList|TestToolRead' -v`
Expected: FAIL

- [ ] **Step 3: Implement model-facing inspection tools**

Rules:
- `skills.list` returns summaries only
- `skills.read` returns one structured detail
- `skills.read` may truncate long prompt bodies

- [ ] **Step 4: Run focused tests**

Run: `go test ./internal/skills -run 'TestToolList|TestToolRead' -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/skills/tools.go internal/skills/tools_test.go
git commit -m "feat: add skills inspection tools"
```

### Task 6: Wire Filesystem Skills Into Coordinator Startup

**Files:**
- Modify: `internal/config/config.go`
- Modify: `internal/config/config_test.go`
- Modify: `cmd/coordinator/main.go`

- [ ] **Step 1: Write the failing config/startup test**

Add a test that expects workspace-root-based skills loader wiring in startup deps.

- [ ] **Step 2: Run focused config test**

Run: `go test ./internal/config -run TestLoad -v`
Expected: FAIL after adding assertion

- [ ] **Step 3: Implement loader wiring**

Rules:
- skills root derived from `TEAMD_WORKSPACE_ROOT`
- single-agent owner gets filesystem loader by default

- [ ] **Step 4: Run focused tests**

Run: `go test ./internal/config ./cmd/coordinator`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add internal/config/config.go internal/config/config_test.go cmd/coordinator/main.go
git commit -m "feat: wire filesystem skills into coordinator"
```

### Task 7: Add `/skills` Telegram Commands

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write failing adapter tests for `/skills`**

Test:
- `/skills`
- `/skills list`
- `/skills use deploy`
- `/skills drop deploy`
- `/skills reset`

- [ ] **Step 2: Run focused adapter tests**

Run: `go test ./internal/transport/telegram -run 'TestAdapter.*Skills' -v`
Expected: FAIL

- [ ] **Step 3: Implement reserved `/skills` command handling**

Requirements:
- concise text responses
- unknown skill handling
- active skill state bound to current session key

- [ ] **Step 4: Add command-menu sync coverage**

Ensure `/skills` is in `SyncCommands()`.

- [ ] **Step 5: Run focused tests**

Run: `go test ./internal/transport/telegram -run 'TestAdapter.*Skills|TestAdapterSyncCommands' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: add telegram skills commands"
```

### Task 8: Inject Catalog And Active Skills Into Direct Runtime Prompt

**Files:**
- Modify: `internal/transport/telegram/adapter.go`
- Modify: `internal/transport/telegram/adapter_test.go`

- [ ] **Step 1: Write the failing provider-request test**

```go
func TestAdapterReplyInjectsActiveSkillsIntoProviderPrompt(t *testing.T) {
    // activate skill for current session
    // send ordinary message
    // assert provider request contains AGENTS.md context + available-skills catalog + active skill prompt
}
```

- [ ] **Step 2: Run focused test**

Run: `go test ./internal/transport/telegram -run TestAdapterReplyInjectsActiveSkillsIntoProviderPrompt -v`
Expected: FAIL

- [ ] **Step 3: Implement prompt layering**

Rules:
- `AGENTS.md` first
- then available-skills catalog
- then active skills prompt
- then user/system conversation payload

- [ ] **Step 4: Add test for per-session isolation**

- [ ] **Step 5: Run focused tests**

Run: `go test ./internal/transport/telegram -run 'TestAdapterReplyInjectsActiveSkillsIntoProviderPrompt|TestAdapterSkillsAreSessionScoped' -v`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add internal/transport/telegram/adapter.go internal/transport/telegram/adapter_test.go
git commit -m "feat: inject active skills into direct runtime prompt"
```

### Task 9: Add Example Skill and Docs

**Files:**
- Create: `skills/example/SKILL.md`
- Modify: `skills/README.md`

- [ ] **Step 1: Add a minimal example skill**

Example should demonstrate:
- metadata header
- `version`
- prompt body
- naming conventions

- [ ] **Step 2: Document directory structure and Telegram commands**

Include:
- where skills live
- how bot discovers them
- difference between prompt catalog and active skill injection
- `skills.list/read` as model-facing inspection tools
- command examples
- prompt budget caveat

- [ ] **Step 3: Run verification**

Run:
```bash
go test ./internal/skills ./internal/transport/telegram
go test ./...
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add skills/example/SKILL.md skills/README.md
git commit -m "docs: add example skill bundle"
```

## Recommended Execution Order

1. Task 1: metadata parsing
2. Task 2: filesystem catalog
3. Task 3: session skill state
4. Task 4: bounded prompt composition
5. Task 5: skills inspection tools
6. Task 6: coordinator wiring
7. Task 7: Telegram `/skills`
8. Task 8: direct prompt injection
9. Task 9: example + docs

## Risks To Watch

- Prompt blow-up if multiple verbose skills are enabled together
- Ambiguous naming if two directories resolve to the same skill name
- Session-state drift if active session key handling changes
- Overloading the bot with automatic skill behavior too early
- Confusion between inspection and activation if `skills.read` semantics are not kept strict

## Follow-Ups After MVP

- Automatic skill suggestion based on user intent
- Skill dependencies and nested references
- Script/templates/assets inside skill folders
- Mesh-aware skill delegation
- Per-skill enable/disable policy in `OrchestrationPolicy`
