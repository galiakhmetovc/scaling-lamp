# Memory Policy Cookbook

This is an operator-facing guide for `TEAMD_*` memory presets.

The important detail: the runtime does **not** switch behavior based on `TEAMD_MEMORY_POLICY_PROFILE` alone. The actual behavior comes from the full set of knobs:

- `TEAMD_MEMORY_POLICY_PROFILE`
- `TEAMD_MEMORY_PROMOTE_CHECKPOINT`
- `TEAMD_MEMORY_PROMOTE_CONTINUITY`
- `TEAMD_MEMORY_RECALL_KINDS`
- `TEAMD_MEMORY_MAX_BODY_CHARS`
- `TEAMD_MEMORY_MAX_RESOLVED_FACTS`

Use the blocks below as complete presets. Paste one profile as a unit, then verify it with `/runtime` or `/memory policy`.

## 1) Conservative default

Use this for normal production traffic when you want stable recall without turning memory into a transcript dump.

```env
TEAMD_MEMORY_POLICY_PROFILE=conservative
TEAMD_MEMORY_PROMOTE_CHECKPOINT=false
TEAMD_MEMORY_PROMOTE_CONTINUITY=true
TEAMD_MEMORY_RECALL_KINDS=continuity
TEAMD_MEMORY_MAX_BODY_CHARS=600
TEAMD_MEMORY_MAX_RESOLVED_FACTS=3
```

When to use it:

- Day-to-day operator runs
- Mixed traffic where tool output is noisy
- Any case where you care more about memory quality than memory coverage

What can go wrong:

- The agent may forget older session details that only lived in checkpoint material
- Long investigations may need manual `memory search` / `memory read`
- If continuity is weak, the agent will look "forgetful" even though the policy is working

## 2) Retrieval-heavy profile

Use this when the task spans many turns, several handoffs, or repeated revisits to the same facts.

```env
TEAMD_MEMORY_POLICY_PROFILE=retrieval-heavy
TEAMD_MEMORY_PROMOTE_CHECKPOINT=true
TEAMD_MEMORY_PROMOTE_CONTINUITY=true
TEAMD_MEMORY_RECALL_KINDS=continuity,checkpoint
TEAMD_MEMORY_MAX_BODY_CHARS=1200
TEAMD_MEMORY_MAX_RESOLVED_FACTS=6
```

When to use it:

- Long debugging sessions
- Multi-step investigations with lots of context reuse
- Situations where a later run needs both "what happened" and "what matters now"

What can go wrong:

- Searchable memory gets noisier if checkpoint text is sloppy
- More recall means more chance of dragging stale details into the prompt
- Bigger bodies can crowd out newer context if the underlying session is already noisy

Operational note:

- This profile is usually safer if embeddings are enabled, because the wider recall surface is more useful when vector search is available.

## 3) Local operator-heavy profile

Use this when a human operator is actively steering the session and you want the runtime to stay compact and inspectable.

```env
TEAMD_MEMORY_POLICY_PROFILE=local-operator-heavy
TEAMD_MEMORY_PROMOTE_CHECKPOINT=false
TEAMD_MEMORY_PROMOTE_CONTINUITY=true
TEAMD_MEMORY_RECALL_KINDS=continuity
TEAMD_MEMORY_MAX_BODY_CHARS=400
TEAMD_MEMORY_MAX_RESOLVED_FACTS=2
```

When to use it:

- Interactive debugging from the local console
- Sessions where the operator will manually inspect memory and artifacts
- Tasks where concise continuity is more useful than broad automatic recall

What can go wrong:

- The agent may ask the operator to restate details that would have been preserved in checkpoint memory
- Useful context can be trimmed too aggressively if the current state is already compact
- If the operator expects "smart retrieval," this profile will feel too bare

Operator habit that pairs well with it:

- Check `/runtime` or `/memory policy` before a long session
- Use `teamd-agent memory search <chat_id> <session_id> <query>` when the model seems to have lost a detail
- Use `teamd-agent memory read <doc_key>` for the exact document behind a recall hit

## 4) Debug / investigation profile

Use this only when you need the widest practical recall surface for a short period and you are willing to inspect the output closely.

```env
TEAMD_MEMORY_POLICY_PROFILE=debug-investigation
TEAMD_MEMORY_PROMOTE_CHECKPOINT=true
TEAMD_MEMORY_PROMOTE_CONTINUITY=true
TEAMD_MEMORY_RECALL_KINDS=continuity,checkpoint
TEAMD_MEMORY_MAX_BODY_CHARS=1600
TEAMD_MEMORY_MAX_RESOLVED_FACTS=8
```

When to use it:

- Reproducing a tricky bug
- Comparing several runs against the same session history
- Investigating a bad compaction or recall decision

What can go wrong:

- This is the easiest way to pollute searchable memory with borderline text
- It can hide the real issue by making the model seem "better" only because it sees more noise
- If left on permanently, it tends to accumulate stale or contradictory recall

## Anti-pattern: hoard raw history

Do not treat memory as a dump of everything the run saw.

```env
TEAMD_MEMORY_POLICY_PROFILE=anti-pattern
TEAMD_MEMORY_PROMOTE_CHECKPOINT=true
TEAMD_MEMORY_PROMOTE_CONTINUITY=true
TEAMD_MEMORY_RECALL_KINDS=continuity,checkpoint
TEAMD_MEMORY_MAX_BODY_CHARS=5000
TEAMD_MEMORY_MAX_RESOLVED_FACTS=50
```

Why this is bad:

- Raw checkpoint text is often noisy, repetitive, or partial
- Large bodies increase prompt bloat and slow down the useful signal
- More facts do not mean better recall if the facts were never clean

Typical failure modes:

- The model starts "remembering" stale tool output instead of durable facts
- Search results look impressive but are hard to trust
- Compaction has to work harder just to clean up the memory policy mistake

If you need more recall, raise quality first:

1. Promote only the kinds you can defend.
2. Keep bodies short enough to read.
3. Keep resolved facts small and current.
4. Verify recall with real operator workflows, not just by looking at the env block.

## Quick selection rule

- Choose `conservative` if you are unsure.
- Choose `retrieval-heavy` if the run spans many turns and repeated context reuse matters.
- Choose `local-operator-heavy` if a human is doing frequent manual steering.
- Choose `debug-investigation` only for short-lived troubleshooting.

## Related surfaces

- `docs/agent/05-memory-and-recall.md`
- `docs/agent/02-bootstrap-and-config.md`
- `internal/runtime/memory_policy.go`
- `internal/runtime/session_overrides.go`
- `internal/transport/telegram/runtime_commands.go`
