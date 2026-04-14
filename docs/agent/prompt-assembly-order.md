# Prompt Assembly Order

This document describes the exact prompt construction path used by the single-agent runtime.

## The Order

The request path is:

1. Load raw session messages from the conversation store.
2. If compaction is needed, compact the older transcript prefix and save a checkpoint.
3. Re-load the session messages so the newest stored state is what gets assembled.
4. Prune old prompt residency without rewriting durable transcript state.
5. Build the base prompt with `compaction.AssemblePrompt(...)`.
6. Inject runtime-owned prompt fragments with `runtime.PromptContextAssembler.Build(...)`.
6. Send the resulting message list to the provider.

The code path for this is:

- [internal/runtime/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context.go)
- [internal/compaction/assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/compaction/assembler.go)
- [internal/runtime/pruning.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/pruning.go)
- [internal/runtime/prompt_context_assembler.go](/home/admin/AI-AGENT/data/projects/teamD/internal/runtime/prompt_context_assembler.go)
- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)

## Where Compaction Happens

Compaction happens before prompt assembly in `prepareConversationRound(...)`:

- `maybeCompactConversation(...)` checks the budget and, if needed, calls the compaction service.
- the trigger uses projected final prompt size, not only raw transcript size
- The compaction service writes a checkpoint back to the session store.
- `compaction.AssemblePrompt(...)` then picks up that checkpoint and inserts it into the base prompt.

Important detail: the checkpoint is part of the compaction layer, not the runtime prompt-context injector.

## What `PromptContextAssembler` Injects

`internal/runtime/prompt_context_assembler.go` injects runtime-owned system blocks in this fixed order:

1. workspace context
2. SessionHead
3. recent-work follow-up guidance
4. memory recall
5. skills catalog
6. active skills

Only after those blocks are added does the assembler append the assembled session messages.

The `recent_work` layer is only injected for underspecified follow-up requests such as "continue" or "save this as a project". It is built from `SessionHead` and tells the model to treat the most recent completed run, its result summary, and recent artifacts as the primary source of truth before broader memory recall.

The Telegram adapter wires those hooks here:

- [internal/transport/telegram/conversation.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/conversation.go)
- [internal/transport/telegram/prompt_context.go](/home/admin/AI-AGENT/data/projects/teamD/internal/transport/telegram/prompt_context.go)

## What Is Not Included

The prompt path does not add any of these directly:

- provider config or runtime request config
- tool execution output that is not already stored in session history
- approvals, traces, or run-state UI data
- memory documents themselves
- skills implementation files or filesystem contents

`PromptContextAssembler` is deliberately narrow: it only adds the workspace, recall, and skills prompt fragments that the runtime has already prepared through hooks.

## Why This Matters

The prompt the provider sees is not just the raw transcript. It is:

1. pruned raw transcript
2. compaction checkpoint + selected preserved tail/prefix
3. runtime-owned system fragments
4. the final assembled message list sent to the provider

That ordering matters because prompt budget is now projected from the final assembled prompt, not guessed only from raw history. See also [context-budget.md](/home/admin/AI-AGENT/data/projects/teamD/docs/agent/context-budget.md).
