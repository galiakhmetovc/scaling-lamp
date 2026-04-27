# Jaeger Auto Export Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a Jaeger web UI container and optional automatic OTLP export of local `trace_links` after each completed runtime run.

**Architecture:** Keep `agentd` as the source of truth: traces remain in SQLite `trace_links`, and the exporter sends a bounded OTel-compatible JSON projection to an external backend. The container add-on script deploys Jaeger all-in-one with persistent Badger storage and Caddy routes; runtime export is controlled by config/env and is best-effort.

**Tech Stack:** Rust, SQLite trace_links, reqwest blocking client, Docker Compose, Caddy, Jaeger all-in-one OTLP HTTP.

---

### Task 1: Deploy Jaeger Add-On

**Files:**
- Modify: `scripts/deploy-teamd-containers.sh`
- Modify: `scripts/test-deploy-teamd.sh`
- Modify: `docs/current/14-container-addons.md`

- [x] Add failing deploy-script assertions for `--with-jaeger`, `teamd-jaeger`, `16686`, `4318`, `/jaeger/`, and env upserts.
- [x] Add script options and environment variables for Jaeger image, UI port, OTLP ports, Badger storage path, and Caddy routing.
- [x] Write a Jaeger compose file using `jaegertracing/all-in-one`, `SPAN_STORAGE_TYPE=badger`, `COLLECTOR_OTLP_ENABLED=true`, and persistent volume directories.
- [x] Configure `/etc/teamd/teamd.env` with OTLP exporter defaults when Jaeger is enabled.
- [x] Update Caddyfile generation to expose `/jaeger/` or `jaeger.<domain>`.
- [x] Document local and domain URLs plus operational checks.

### Task 2: Runtime OTLP Exporter

**Files:**
- Modify: `crates/agent-persistence/src/config.rs`
- Create: `cmd/agentd/src/otel.rs`
- Modify: `cmd/agentd/src/lib.rs`
- Modify: `cmd/agentd/src/bootstrap/execution_ops.rs`
- Modify: `cmd/agentd/src/execution/chat.rs`
- Modify: `cmd/agentd/src/cli/render.rs`
- Test: `cmd/agentd/src/cli/tests.rs`
- Test: `cmd/agentd/tests/tool_call_smoke.rs`

- [x] Add failing config tests for `[observability] otlp_export_enabled`, `otlp_endpoint`, `otlp_timeout_ms` and env overrides.
- [x] Extract the current trace JSON projection into reusable runtime code.
- [x] Add `export_trace_to_otlp_http` that POSTs JSON to `/v1/traces` with bounded timeout and no raw transcript/tool payloads.
- [x] Add `teamdctl trace push <trace_id>` for manual export.
- [x] Auto-export after successful chat/background/inter-agent run finalization when config is enabled.
- [x] Emit diagnostic audit events for export success/failure without failing the user turn.

### Task 3: Verification

- [x] Run targeted red/green tests for deploy script, config, trace export, and auto-export.
- [x] Run `cargo fmt --all`.
- [x] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] Run `cargo test --workspace --all-features`.
- [x] Run `cargo build -p agentd`.
- [x] Run `cargo build --release -p agentd`.
