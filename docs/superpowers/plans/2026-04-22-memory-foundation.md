# Memory Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first native memory foundation for teamD with retention tiers, cold-session archival, and canonical bounded retrieval for historical sessions and project knowledge.

**Architecture:** Extend the existing `agent-persistence` store and the canonical runtime/tool path instead of adding a second memory service. Implement session memory first with real retention metadata and archive bundles, then layer bounded search/read tools and finally add project-knowledge indexing on the same substrate.

**Tech Stack:** Rust, SQLite/rusqlite, existing `agent-persistence` repositories, existing `agent-runtime` tool surface, daemon-backed `agentd`

---

## File Map

### New files

- Create: `docs/superpowers/specs/2026-04-22-memory-foundation-design.md`
- Create: `docs/superpowers/plans/2026-04-22-memory-foundation.md`
- Create: `crates/agent-runtime/src/archive.rs`
- Create: `crates/agent-persistence/src/store/memory_repos.rs`
- Create: `cmd/agentd/src/execution/memory.rs`

### Existing files to modify

- Modify: `crates/agent-runtime/src/lib.rs`
- Modify: `crates/agent-runtime/src/memory.rs`
- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/tests.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/tests.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/execution/tools.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/context.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/chat.rs`

## Task 1: Retention Metadata Foundation

**Files:**

- Modify: `crates/agent-runtime/src/memory.rs`
- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Create: `crates/agent-persistence/src/store/memory_repos.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write the failing store test for session retention round-trip**

Add a test proving the store can persist and load:

- `session_id`
- `tier = active | warm | cold`
- `last_accessed_at`
- `archived_at`
- `archive_manifest_path`
- `archive_version`
- `updated_at`

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
cargo test -p agent-persistence session_retention -- --nocapture
```

Expected:

- failure because the retention record type/repository/schema does not exist yet

- [ ] **Step 3: Add runtime retention types**

Add minimal types in `crates/agent-runtime/src/memory.rs`:

- `SessionRetentionTier`
- `SessionRetentionState`

Do not add search or archive logic yet.

- [ ] **Step 4: Add persistence record and repository interfaces**

Add:

- `SessionRetentionRecord`
- `SessionRetentionRepository`

Keep the API minimal:

- `put_session_retention`
- `get_session_retention`
- `list_session_retentions`

- [ ] **Step 5: Add schema and repository implementation**

Create `session_retention` table and repository implementation in `memory_repos.rs`.

- [ ] **Step 6: Run the focused test to verify it passes**

Run:

```bash
cargo test -p agent-persistence session_retention -- --nocapture
```

Expected:

- PASS

- [ ] **Step 7: Commit**

```bash
git add crates/agent-runtime/src/memory.rs crates/agent-persistence/src/repository.rs crates/agent-persistence/src/records.rs crates/agent-persistence/src/store.rs crates/agent-persistence/src/store/schema.rs crates/agent-persistence/src/store/memory_repos.rs crates/agent-persistence/src/store/tests.rs
git commit -m "feat: add session retention metadata"
```

## Task 2: Cold Session Archive Bundles

**Files:**

- Create: `crates/agent-runtime/src/archive.rs`
- Modify: `crates/agent-runtime/src/lib.rs`
- Modify: `crates/agent-persistence/src/store.rs`
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write the failing archive bundle round-trip test**

Write a test that:

- creates a session with transcripts, summary, and artifact refs
- archives it into `data_dir/archives/sessions/<session_id>/`
- reads the manifest back

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
cargo test -p agent-persistence archive_bundle -- --nocapture
```

Expected:

- failure because no archive writer/reader exists yet

- [ ] **Step 3: Add minimal archive manifest types**

Add:

- `SessionArchiveManifest`
- `ArchivedArtifactEntry`

- [ ] **Step 4: Implement archive writer and reader**

Keep the first pass simple:

- write `manifest.json`
- write `summary.json`
- write `transcript.ndjson`
- copy referenced artifacts into archive bundle

- [ ] **Step 5: Run the focused test to verify it passes**

Run:

```bash
cargo test -p agent-persistence archive_bundle -- --nocapture
```

Expected:

- PASS

## Task 3: Session Search Index

**Files:**

- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/memory_repos.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write the failing search index test**

Write a test proving:

- session title
- context summary
- transcript text

can be indexed and searched by query.

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
cargo test -p agent-persistence session_search_index -- --nocapture
```

Expected:

- failure because session search docs and FTS table do not exist yet

- [ ] **Step 3: Add search-doc records and repository methods**

Add minimal session search doc records and FTS-backed repository methods.

- [ ] **Step 4: Add schema and implementation**

Create:

- `session_search_docs`
- `session_search_fts`

- [ ] **Step 5: Run the focused test to verify it passes**

Run:

```bash
cargo test -p agent-persistence session_search_index -- --nocapture
```

Expected:

- PASS

## Task 4: Canonical `session_search` Tool

**Files:**

- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/tests.rs`
- Modify: `cmd/agentd/src/execution.rs`
- Create: `cmd/agentd/src/execution/memory.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Modify: `cmd/agentd/src/execution/tools.rs`
- Test: `cmd/agentd/tests/bootstrap_app/chat.rs`

- [ ] **Step 1: Write the failing runtime tool test**

Write a test proving `session_search`:

- accepts a query
- returns bounded rows
- returns `next_cursor` when truncated

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
cargo test -p agentd session_search -- --nocapture
```

Expected:

- failure because the tool is not yet defined or wired

- [ ] **Step 3: Add tool schema and output types**

Define:

- `SessionSearchInput`
- `SessionSearchOutput`

- [ ] **Step 4: Implement execution-layer handler**

Implement the canonical handler in `cmd/agentd/src/execution/memory.rs`.

- [ ] **Step 5: Run the focused test to verify it passes**

Run:

```bash
cargo test -p agentd session_search -- --nocapture
```

Expected:

- PASS

## Task 5: Canonical `session_read` Tool

**Files:**

- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/tests.rs`
- Modify: `cmd/agentd/src/execution/memory.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Test: `cmd/agentd/tests/bootstrap_app/context.rs`

- [ ] **Step 1: Write the failing runtime tool test**

Write a test proving `session_read`:

- can read `summary`
- can read `timeline`
- can read bounded transcript chunks
- marks `from_archive` correctly for cold sessions

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
cargo test -p agentd session_read -- --nocapture
```

Expected:

- failure because the tool is not yet defined or implemented

- [ ] **Step 3: Add tool schema and output types**

Define:

- `SessionReadInput`
- `SessionReadOutput`

- [ ] **Step 4: Implement warm/cold read path**

Use primary store for warm sessions and archive hydration for cold sessions.

- [ ] **Step 5: Run the focused test to verify it passes**

Run:

```bash
cargo test -p agentd session_read -- --nocapture
```

Expected:

- PASS

## Task 6: Knowledge Index Metadata

**Files:**

- Modify: `crates/agent-persistence/src/repository.rs`
- Modify: `crates/agent-persistence/src/records.rs`
- Modify: `crates/agent-persistence/src/store/schema.rs`
- Modify: `crates/agent-persistence/src/store/memory_repos.rs`
- Test: `crates/agent-persistence/src/store/tests.rs`

- [ ] **Step 1: Write the failing knowledge source test**

Write a test proving canonical knowledge roots can be indexed by path and metadata.

- [ ] **Step 2: Run the focused test to verify it fails**

Run:

```bash
cargo test -p agent-persistence knowledge_source -- --nocapture
```

Expected:

- failure because knowledge source records/schema do not exist yet

- [ ] **Step 3: Add source records and repository methods**

Add:

- `KnowledgeSourceRecord`
- `KnowledgeSearchDocRecord`

- [ ] **Step 4: Add schema and implementation**

Create:

- `knowledge_sources`
- `knowledge_search_docs`
- `knowledge_search_fts`

- [ ] **Step 5: Run the focused test to verify it passes**

Run:

```bash
cargo test -p agent-persistence knowledge_source -- --nocapture
```

Expected:

- PASS

## Task 7: Canonical `knowledge_search` and `knowledge_read`

**Files:**

- Modify: `crates/agent-runtime/src/tool.rs`
- Modify: `crates/agent-runtime/src/tool/tests.rs`
- Modify: `cmd/agentd/src/execution/memory.rs`
- Modify: `cmd/agentd/src/execution/provider_loop.rs`
- Test: `cmd/agentd/tests/bootstrap_app/chat.rs`

- [ ] **Step 1: Write the failing runtime tool tests**

Write tests for:

- `knowledge_search`
- `knowledge_read`

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```bash
cargo test -p agentd knowledge_search -- --nocapture
cargo test -p agentd knowledge_read -- --nocapture
```

Expected:

- failures because the tools are not yet defined or wired

- [ ] **Step 3: Add tool schemas and outputs**

Define the search/read input and output types.

- [ ] **Step 4: Implement canonical handlers**

Use canonical roots first:

- `README.md`
- `SYSTEM.md`
- `AGENTS.md`
- `docs/**`
- `projects/**`
- `notes/**`

- [ ] **Step 5: Run the focused tests to verify they pass**

Run:

```bash
cargo test -p agentd knowledge_search -- --nocapture
cargo test -p agentd knowledge_read -- --nocapture
```

Expected:

- PASS

## Task 8: Background Maintenance and Integration Polish

**Files:**

- Modify: `cmd/agentd/src/execution/background.rs`
- Modify: `cmd/agentd/src/bootstrap/context_ops.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/context.rs`
- Modify: `cmd/agentd/tests/bootstrap_app/chat.rs`

- [ ] **Step 1: Write the failing maintenance tests**

Write tests for:

- retention transition from `active` to `warm`
- explicit archival to `cold`
- stale knowledge reindex behavior

- [ ] **Step 2: Run the focused tests to verify they fail**

Run:

```bash
cargo test -p agentd retention_transition -- --nocapture
```

Expected:

- failure because maintenance hooks do not exist yet

- [ ] **Step 3: Implement the minimal maintenance hooks**

Do not overbuild policy. Add only the hooks needed by tests.

- [ ] **Step 4: Run the focused tests to verify they pass**

Run:

```bash
cargo test -p agentd retention_transition -- --nocapture
```

Expected:

- PASS

## Final Verification

- [ ] Run formatting:

```bash
cargo fmt --all
```

- [ ] Run clippy:

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- [ ] Run full tests:

```bash
cargo test --workspace --all-features
```

- [ ] Run debug build:

```bash
cargo build -p agentd
```

- [ ] Run release build:

```bash
cargo build --release -p agentd
```
