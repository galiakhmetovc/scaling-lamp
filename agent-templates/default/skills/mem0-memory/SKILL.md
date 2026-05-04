---
name: mem0-memory
description: Используй этот skill для долговременной памяти: устойчивые предпочтения пользователя, факты, повторяющиеся исправления, правила поведения, long-term context, durable memory, user preferences, stable facts, recurring corrections and remembered behavior. Не используй для точных настроек, временного состояния, файлов или документации.
---

# Mem0 Memory

Use this skill when the user gives durable semantic context that should be remembered across sessions.

## What belongs in Mem0

- Stable operator preferences, for example language, response style, forbidden approaches, and recurring workflow preferences.
- Durable project context that is useful semantically but is not canonical documentation.
- Repeated corrections and successful patterns that should influence future behavior.
- Facts that can be retrieved approximately by meaning.

## What does not belong in Mem0

- Exact settings, feature flags, selected ids, counters, or small JSON records. Use `scoped-kv` instead.
- Human-readable documentation, project notes, decisions, and wiki pages. Use `silverbullet-space` instead.
- Temporary task state, current plan items, raw files, transcripts, tool outputs, and artifacts.

## Tools

- Use `memory_search` before relying on remembered facts when the answer depends on long-term context.
- Use `memory_add` only for durable facts that the operator would expect the system to remember.
- Use `memory_delete` only when the user explicitly asks to remove a memory or the memory is clearly wrong.
- Use Mem0 pointers to reconnect future sessions to durable SilverBullet notes: include note path, topic, short summary, tags, and why it matters.
- Do not copy whole SilverBullet notes into Mem0. Store the pointer and retrieve/read the note through SilverBullet when details matter.

## Operating rules

1. Keep memories short and factual.
2. Prefer one memory per durable fact.
3. Do not store secrets, payment data, tokens, passwords, or private files.
4. If memory search returns low-confidence or irrelevant results, say that memory did not help and continue from current context.
5. When saving a memory, summarize what was saved.
6. When a memory points to a note, read the note before acting on detailed content.
