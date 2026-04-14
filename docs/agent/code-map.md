# Code Map

## Request Entry

- [cmd/coordinator/main.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/main.go)
- [cmd/coordinator/bootstrap.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/bootstrap.go)
- [cmd/coordinator/cli.go](/home/admin/AI-AGENT/data/projects/teamD/cmd/coordinator/cli.go)
- [docs/agent/request-lifecycle.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/request-lifecycle.md)

## HTTP API And CLI

- [internal/api/server.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/server.go)
- [internal/api/types.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/types.go)
- [internal/api/errors.go](/home/admin/AI-AGENT/data/projects/teamD/internal/api/errors.go)
- [internal/cli/client.go](/home/admin/AI-AGENT/data/projects/teamD/internal/cli/client.go)
- [internal/runtime/runtime_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/runtime_api.go)
  - stable runtime-owned queries for runs, sessions, approvals, and runtime summary
- [internal/runtime/agent_core.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/agent_core.go)
  - canonical orchestration facade for API handlers and transports
- [docs/agent/http-api.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/http-api.md)
- [docs/agent/cli.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/cli.md)
- [docs/agent/agentcore.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/agentcore.md)
- [docs/agent/operator-chat.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/operator-chat.md)
- [docs/agent/state-machines.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/state-machines.md)
- [docs/agent/prompt-assembly-order.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/prompt-assembly-order.md)
- [docs/agent/memory-policy-cookbook.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/memory-policy-cookbook.md)
- [docs/agent/testing.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/testing.md)
- [docs/agent/runtime-api-walkthrough.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/runtime-api-walkthrough.md)
- [docs/agent/approvals.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/approvals.md)
- [docs/agent/jobs.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/jobs.md)
- [docs/agent/workers.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/workers.md)
- [docs/agent/plans.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/plans.md)
- [docs/agent/artifact-offload.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/artifact-offload.md)

## Telegram Transport

- [internal/transport/telegram/adapter.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/adapter.go)
- [internal/transport/telegram/immediate_updates.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/immediate_updates.go)
- [internal/transport/telegram/run_lifecycle.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/run_lifecycle.go)
- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)
- [internal/transport/telegram/runtime_support.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/runtime_support.go)
- [internal/transport/telegram/session_commands.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_commands.go)
- [internal/transport/telegram/runtime_commands.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/runtime_commands.go)
- [internal/transport/telegram/skills_commands.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/skills_commands.go)
- [internal/transport/telegram/mesh_commands.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/mesh_commands.go)
- [internal/transport/telegram/ui_helpers.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/ui_helpers.go)
- [internal/transport/telegram/telegram_api.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/telegram_api.go)
- [internal/transport/telegram/provider_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/provider_tools.go)
- [internal/transport/telegram/delegation_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/delegation_tools.go)
- [internal/transport/telegram/memory_tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_tools.go)
- [internal/transport/telegram/tool_helpers.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/tool_helpers.go)
- [internal/transport/telegram/runtime_guards.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/runtime_guards.go)
- [internal/transport/telegram/memory_runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/memory_runtime.go)
- [internal/transport/telegram/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/prompt_context.go)
- [internal/transport/telegram/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/store.go)
- [internal/transport/telegram/session_transcript_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_transcript_store.go)
- [internal/transport/telegram/session_checkpoint_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_checkpoint_store.go)
- [internal/transport/telegram/session_selector_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/session_selector_store.go)
- [internal/transport/telegram/postgres_transcript_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/postgres_transcript_store.go)
- [internal/transport/telegram/postgres_checkpoint_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/postgres_checkpoint_store.go)
- [internal/transport/telegram/run_state.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/run_state.go)
- [internal/transport/telegram/status_sync.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/status_sync.go)
- [internal/transport/telegram/formatting.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/formatting.go)
- [internal/transport/telegram/status_card.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/status_card.go)

## Run Lifecycle

- [internal/runtime/execution_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/execution_service.go)
- [internal/runtime/run_manager.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/run_manager.go)
- [internal/runtime/active_registry.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/active_registry.go)
- [internal/runtime/conversation_engine.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/conversation_engine.go)
- [internal/runtime/jobs_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/jobs_service.go)
- [internal/runtime/workers_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/workers_service.go)
- [internal/runtime/plans_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/plans_service.go)
- [internal/runtime/handoffs_service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/handoffs_service.go)
- [internal/runtime/memory_documents.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/memory_documents.go)
- [internal/runtime/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
- [internal/runtime/prompt_context_assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context_assembler.go)
- [internal/runtime/replay.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/replay.go)
- [docs/superpowers/specs/2026-04-12-agentcore-facade-design.md](/home/admin/AI-AGENT/data/projects/teamD/docs/superpowers/specs/2026-04-12-agentcore-facade-design.md)
- [docs/superpowers/plans/2026-04-12-agentcore-facade-implementation.md](/home/admin/AI-AGENT/data/projects/teamD/docs/superpowers/plans/2026-04-12-agentcore-facade-implementation.md)
- [internal/runtime/control_actions.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/control_actions.go)
- [internal/runtime/session_actions.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/session_actions.go)
- [internal/runtime/session_overrides.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/session_overrides.go)
- [internal/runtime/policy_resolver.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/policy_resolver.go)
- [internal/runtime/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/store.go)
- [internal/runtime/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/postgres_store.go)

## Provider

- [internal/provider/provider.go](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/provider.go)
- [internal/provider/zai/client.go](/home/admin/AI-AGENT/data/projects/teamD/internal/provider/zai/client.go)

## Tool Runtime

- [internal/mcp/runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/runtime.go)
- [internal/mcp/tools/filesystem.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/tools/filesystem.go)
- [internal/mcp/tools/shell.go](/home/admin/AI-AGENT/data/projects/teamD/internal/mcp/tools/shell.go)

## Skills

- [internal/skills/runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/skills/runtime.go)
- [internal/skills/tools.go](/home/admin/AI-AGENT/data/projects/teamD/internal/skills/tools.go)
- [internal/skills/prompts.go](/home/admin/AI-AGENT/data/projects/teamD/internal/skills/prompts.go)

## Memory

- [internal/memory/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/store.go)
- [internal/memory/postgres_store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/postgres_store.go)
- [internal/memory/recall.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/recall.go)
- [internal/memory/ollama_embedder.go](/home/admin/AI-AGENT/data/projects/teamD/internal/memory/ollama_embedder.go)

## Compaction

- [internal/compaction/budget.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/budget.go)
- [internal/compaction/assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/assembler.go)
- [internal/compaction/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/service.go)

## Trace And Observability

- [internal/llmtrace/trace.go](/home/admin/AI-AGENT/data/projects/teamD/internal/llmtrace/trace.go)
- [internal/observability/logging.go](/home/admin/AI-AGENT/data/projects/teamD/internal/observability/logging.go)

## Launcher

- [scripts/teamd-agentctl](/home/admin/AI-AGENT/data/projects/teamD/scripts/teamd-agentctl)

## Minimal Skeleton

- [examples/minimal-agent/README.md](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/README.md)
- [examples/minimal-agent/main.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/main.go)
- [examples/minimal-agent/provider.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/provider.go)
- [examples/minimal-agent/tools.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/tools.go)
- [examples/minimal-agent/memory.go](/home/admin/AI-AGENT/data/projects/teamD/examples/minimal-agent/memory.go)

## Not Primary, But Supported

- [internal/events/bus.go](/home/admin/AI-AGENT/data/projects/teamD/internal/events/bus.go)
- [internal/approvals/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/approvals/service.go)
  - approval records, callback handling, pending approvals per session
- [internal/runtime/action_policy.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/action_policy.go)
  - какие tools требуют approval
- [internal/artifacts/store.go](/home/admin/AI-AGENT/data/projects/teamD/internal/artifacts/store.go)
- [internal/workspace/context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/workspace/context.go)
- [internal/worker/runtime.go](/home/admin/AI-AGENT/data/projects/teamD/internal/worker/runtime.go)
- [internal/coordinator/service.go](/home/admin/AI-AGENT/data/projects/teamD/internal/coordinator/service.go)
