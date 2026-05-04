---
name: scoped-kv
description: Используй этот skill для точных структурированных значений: настройки, флаги, счетчики, выбранный агент, выбранная сессия, workspace, feature toggles, exact key-value state, settings, counters, small JSON records and structured values that must be retrieved exactly.
---

# Scoped KV

Use this skill for exact structured state that should be read back without semantic guessing.

## What belongs in KV

- Operator/session/workspace/agent settings and small preferences.
- Selected ids such as current agent, workspace, integration, or feature flag.
- Counters, timestamps, locks, small JSON records, and exact values.
- Values that need compare-and-set revision safety.

## What does not belong in KV

- Long notes, documentation, decisions, and research. Use `silverbullet-space`.
- Semantic memories and broad preferences. Use `mem0-memory`.
- Large tool outputs, files, screenshots, transcripts, or artifacts.

## Tools

- `kv_get` reads one exact key.
- `kv_put` writes one exact key and can use `expected_revision` to avoid lost updates.
- `kv_list` lists keys by prefix inside a scope.
- `kv_delete` removes one key only when deletion is intended.

## Scope rules

- `operator` is for one human operator.
- `agent` is private to one agent profile.
- `agent_shared` is shared by agents on the same runtime node and is the preferred exact-state channel for same-VM inter-agent coordination.
- `workspace` is for a workspace/project.
- `session` is for one session only.

Choose the narrowest scope that still matches the user's intent.

## Inter-agent coordination

- Use `agent_shared` for small exact records that several local agents must read, for example selected environment, shared feature flag, coordination lock, or current handoff id.
- Keep values small JSON objects with explicit fields such as `owner`, `updated_at`, `expires_at`, and `reason`.
- Use `expected_revision` when two agents may update the same key.
- Do not use KV as a chat bus, transcript store, file store, or semantic memory. Use sessions, artifacts, SilverBullet, or Mem0 instead.
