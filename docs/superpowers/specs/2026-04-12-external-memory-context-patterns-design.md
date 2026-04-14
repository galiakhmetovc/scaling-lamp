# External Memory And Context Patterns For teamD

This document compares `teamD` with several external agent systems specifically through the lens of:

- session state
- recent-context handling
- long-term memory
- prompt assembly
- budget accounting
- compaction and pruning
- operator visibility

The goal is not to imitate any one framework. The goal is to extract the minimum set of patterns that solve the concrete problems we are seeing in `teamD`:

- the agent appears to "forget" work that was just completed in the same session
- important context falls out earlier than the operator expects
- prompt budget usage is hard to see from the outside
- different context layers exist, but they do not yet form one clear mental model

## Why This Matters For teamD

`teamD` already has strong primitives:

- raw session history
- checkpoint and continuity
- searchable memory
- compaction
- artifact offload
- replay
- SessionHead baseline

The current weakness is not the absence of memory. The weakness is that the layers are not yet strict enough in role and visibility.

In particular:

- raw transcript exists, but it is too low-level to serve as the canonical recent-context surface
- prompt budgeting exists, but it is mostly invisible and uses rough estimates
- compaction exists, but pruning and selection are still crude
- recent-session truth exists in pieces, but only recently started to become first-class through `SessionHead`

This document answers one question:

What should `teamD` borrow from other systems to become a more reliable single-agent runtime before pushing further into TUI, Telegram UX, and mesh?

## Sources

Primary sources used for this comparison:

- LangGraph persistence docs:
  - https://docs.langchain.com/oss/python/langgraph/persistence
- Anthropic Claude Code memory docs and engineering guidance:
  - https://docs.anthropic.com/en/docs/claude-code/memory
  - https://www.anthropic.com/engineering/claude-code-best-practices
- OpenHands docs:
  - https://docs.all-hands.dev/usage/prompting/microagents-overview
  - https://docs.openhands.dev/sdk/guides/skill
  - https://docs.all-hands.dev/openhands/usage/run-openhands/cli-mode
- OpenClaw docs:
  - https://docs.openclaw.ai/concepts/context
  - https://docs.openclaw.ai/concepts/system-prompt
  - https://docs.openclaw.ai/concepts/session-pruning
  - https://docs.openclaw.ai/concepts/context-engine
- OpenFang public repo and release notes:
  - https://github.com/RightNow-AI/openfang
  - https://raw.githubusercontent.com/RightNow-AI/openfang/main/README.md
  - https://newreleases.io/project/github/RightNow-AI/openfang/release/v0.2.0
  - https://newreleases.io/project/github/RightNow-AI/openfang/release/v0.2.6
- SitePoint article on compression:
  - https://www.sitepoint.com/optimizing-token-usage-context-compression-techniques/

Notes:

- For OpenFang, some conclusions below are marked as inference because the public signals available in README and release notes expose architecture shape but not always the exact implementation.
- For OpenClaw, the public docs are more explicit and therefore the conclusions are stronger.

## The Real Problem In teamD

Before comparing systems, it is important to name the actual failure mode clearly.

The problem is not:

- "we need more memory"
- "we need a bigger context window"
- "the model randomly forgets"

The problem is:

- `teamD` historically relied on transcript, checkpoint, continuity, recall, and files as separate context sources
- but it did not have a sufficiently strong canonical recent-context layer between runs
- therefore, when a new run started, the runtime sometimes reconstructed the session state from the wrong source order
- this made the agent appear to forget work that had just happened

That is why `SessionHead` is directionally correct.

The next question is whether this is enough on its own. The answer is no.

`SessionHead` fixes one missing layer, but the broader system still needs:

- prompt budget transparency
- projected budget accounting, not just raw transcript estimates
- better selection/pruning before compaction
- clearer separation of always-loaded, trigger-loaded, and on-demand context

The systems below help show the shape of that next step.

## Comparison Frame

To avoid shallow framework tourism, the comparison uses six concrete questions:

1. What is the canonical unit of recent context?
2. How is long-term memory represented?
3. How is prompt assembly structured?
4. How is token or budget pressure handled?
5. How observable is context composition to an operator?
6. What patterns are directly useful for `teamD`?

## LangGraph

### What It Does Well

LangGraph treats thread state and checkpoints as first-class runtime concepts.

The important part is not the library surface itself. The important part is the architectural stance:

- a run advances through a stateful graph
- checkpoints are persisted deliberately
- future execution resumes from explicit state, not from "re-reading history and hoping reconstruction is correct"

This is highly relevant to `teamD`.

### Canonical Recent Context

In LangGraph, the canonical recent context is not the raw prompt transcript. It is the graph state plus checkpoints.

That is the exact class of thing `teamD` has been missing.

`SessionHead` is essentially a lightweight runtime-native version of this idea:

- last meaningful result
- current goal
- recent artifacts
- unresolved loops
- current project context

### Long-Term Memory

LangGraph itself is not opinionated about project-memory files in the same way as Claude Code or OpenClaw. The main strength here is not long-term memory UX. The strength is persisted execution state.

### Prompt Assembly

The key lesson is that prompt assembly should not be the sole source of continuity. Prompt assembly is downstream of state. State should exist first.

### Budget Handling

LangGraph is less relevant as a budget transparency example than OpenClaw, but it is highly relevant as a "state before prompt" example.

### What teamD Should Take

Direct takeaways:

- `SessionHead` should become a first-class persisted session object, not just a prompt fragment
- continue/recent-work behaviors should resolve through session state, not through transcript heuristics or memory search
- replay and project capture should consume canonical session/run state first

### What teamD Should Not Copy Blindly

- `teamD` does not need to become graph-first internally just to gain state clarity
- a smaller runtime can get most of the value with a canonical session state object and disciplined update rules

## Claude Code

### What It Does Well

Claude Code is strong at one specific thing:

- durable project memory through files

The memory docs and engineering guidance make a clear assumption:

- long-lived truth belongs in files
- not in transient conversation state

This aligns with `teamD` and validates the `project-knowledge-layout` direction.

### Canonical Recent Context

Claude Code does not primarily solve recent-context continuity through a rich persisted runtime object in the same way LangGraph suggests. Instead, it leans hard on:

- files
- stable conventions
- startup loading

That works well for project knowledge, but it is not enough for run-to-run recent truth inside a long active session.

### Long-Term Memory

This is the strongest Claude Code lesson for `teamD`:

- project memory should be explicit
- directory layout matters
- canonical files matter
- retrieval from files is better than vague semantic recollection

In other words:

- `README.md`
- `docs/architecture.md`
- `docs/decisions.md`
- `state/current.md`
- `notes/YYYY-MM-DD.md`

are not "nice to have". They are operational infrastructure.

### Prompt Assembly

Claude Code-style systems often autoload memory files and bootstrap files. The useful lesson here is not "load more files." The useful lesson is:

- know exactly which files are always-loaded
- keep them stable
- keep them short

This maps directly onto `teamD`'s need to distinguish always-loaded from on-demand context.

### Budget Handling

Claude Code guidance implicitly favors keeping always-loaded memory small and structured rather than giant. This is good discipline for `SessionHead` and for skills metadata surfaces.

### What teamD Should Take

- keep project truth in files
- define a stable project layout
- minimize always-loaded bootstrap text
- treat recent-session state and project memory as different layers

### What teamD Should Not Copy Blindly

- file-based memory is not enough to solve recent-run continuity by itself
- `teamD` still needs runtime-native recent-context state, not just files

## OpenHands

### What It Does Well

OpenHands is useful here because it makes the cost of "always loaded instructions" more obvious.

Its microagent model effectively creates context classes:

- always-loaded
- trigger-loaded
- on-demand

That distinction matters a lot.

### Canonical Recent Context

OpenHands is not the clearest model for session-state architecture compared with LangGraph or OpenFang. Its strongest lesson for `teamD` is not canonical session state. Its strongest lesson is prompt-layer classification.

### Long-Term Memory

OpenHands supports structured instructions and microagents, but the most relevant lesson is how aggressively prompt surfaces can bloat if too much is loaded eagerly.

### Prompt Assembly

This is where OpenHands is most useful for `teamD`.

`teamD` today still risks turning skills and workspace context into prompt ballast. Even if each individual layer seems reasonable, the total can become poor:

- workspace bootstrap
- SessionHead
- memory recall
- skills catalog
- active skills
- raw tail
- checkpoint

Without clear classes, the system keeps loading context that competes with the actual task.

### Budget Handling

OpenHands makes a practical point:

- the question is not just "how much context do we have?"
- the real question is "which context belongs in the always-loaded slice?"

### What teamD Should Take

Split prompt context into three classes:

- always-loaded
- trigger-loaded
- on-demand

Concrete candidates in `teamD`:

- Always-loaded:
  - minimal workspace bootstrap
  - SessionHead
  - active session policy summary
- Trigger-loaded:
  - project-specific skill summaries
  - continuity recall blocks
  - current plan summary
- On-demand:
  - large skill docs
  - memory documents
  - artifacts
  - long prior notes

### What teamD Should Not Copy Blindly

- `teamD` should not solve this by adding more skill machinery first
- the main need is classification and budgeting, not more dynamic prompt features

## OpenClaw

### What It Does Well

OpenClaw is the strongest comparison point for prompt/context observability and prompt hygiene.

It appears to do several things better than `teamD` currently does:

- explicit context inspection
- explicit system prompt composition
- separate pruning layer in addition to compaction
- strong visibility into which sources consume tokens

### Canonical Recent Context

OpenClaw is less important as a session-state model than as a context transparency model.

### Long-Term Memory

OpenClaw treats context and memory as separate concerns. This is useful.

`teamD` currently has the right pieces for this separation, but not enough operator-facing clarity.

### Prompt Assembly

OpenClaw is explicit about:

- bootstrap files
- tool schema contribution
- skills metadata
- session contribution

This is exactly the kind of breakdown `teamD` needs.

### Budget Handling

This is the most directly useful part.

OpenClaw separates:

- compaction
- pruning

That distinction matters.

Compaction:

- rewrites older conversation into a shorter summary object

Pruning:

- strips or reduces old prompt baggage for a given model invocation
- without necessarily changing the durable underlying conversation store

`teamD` currently has compaction, but no comparably explicit pruning layer.

That is one reason why context may feel like it disappears too early:

- old material is either still there and bloating the estimate
- or it gets dropped by crude recency-based fitting
- but there is no dedicated middle layer that says "this remains in history, but it does not deserve full prompt residency right now"

### Operator Visibility

OpenClaw also validates a major `teamD` gap:

- operators need to see context composition
- not just total estimated tokens

For `teamD`, this means status surfaces should show:

- full window
- prompt budget
- raw transcript estimate
- system-injected estimate
- checkpoint size
- SessionHead size
- memory recall size
- skills size
- tail size

Without this, context loss feels arbitrary.

### What teamD Should Take

- explicit `/context`-style breakdown
- separate pruning layer
- explicit caps for bootstrap and skills metadata
- stronger operator visibility for prompt composition

### What teamD Should Not Copy Blindly

- `teamD` does not need to reproduce the entire OpenClaw context engine abstraction immediately
- the first win is observability plus pruning, not a plugin architecture

## OpenFang

### What It Does Well

OpenFang is the most interesting comparison point for canonical session state and budget being treated as a system function.

Public signals strongly indicate:

- canonical sessions
- compaction
- budget tracking
- runtime budget APIs

This makes OpenFang especially relevant to the direction `teamD` is already moving toward.

### Canonical Recent Context

This is the big one.

OpenFang appears to treat "canonical session" as a first-class object, rather than letting continuity emerge from transcript and heuristics.

That is the same category of missing piece that led to the `SessionHead` work in `teamD`.

The release note mentioning that a model switch clears canonical session to prevent memory poisoning is especially valuable. It implies:

- session state is explicit
- session state can become incompatible or unsafe across provider/model shifts
- session state lifecycle is managed deliberately

That is a mature idea.

### Long-Term Memory

OpenFang appears to combine SQLite-backed persistence and embeddings with canonical sessions. This suggests a layered model:

- active canonical session
- persisted durable store
- searchable memory

That is a healthy shape.

### Prompt Assembly

One of the more interesting OpenFang ideas is moving canonical context out of a volatile, ever-changing system prompt and into a more stable assembly shape to improve prompt caching.

Even if `teamD` does not adopt this immediately, the principle is important:

- keep the most cache-sensitive layers stable
- keep volatile recent-state layers narrow

### Budget Handling

OpenFang's budget tracking appears to live at a kernel/system level, not inside one adapter's local accounting.

That is where `teamD` should head eventually:

- prompt budget is not just a Telegram UI metric
- it is a runtime contract

### Operator Visibility

The budget API signal suggests that budget is a configurable, inspectable system resource. That is stronger than `teamD`'s current local estimate model.

### What teamD Should Take

- canonical session as first-class runtime object
- budget tracking as runtime subsystem
- session-state hygiene across model/runtime shifts
- eventual distinction between stable prompt layers and volatile recent-context layers

### What teamD Should Not Copy Blindly

- prompt caching optimizations should come after clarity and correctness
- first make state canonical and visible, then optimize assembly for caching

## SitePoint Compression Article

This is not a framework, but it contributes one useful tactical point:

- compression should be staged

The article distinguishes:

- selection
- extraction

and recommends:

- selection first
- extraction second

This matters for `teamD`'s future old-prefix handling.

Today, `teamD` mostly does:

- checkpoint summary
- recent-tail preservation
- recency-based fitting of older prefix

What it lacks is a proper intermediate stage for older context:

- select which older blocks deserve residency
- then extract only the most useful lines from those blocks

This is likely the right future direction for recall-heavy or long-lived sessions.

## Cross-System Pattern Map

### Pattern 1: Canonical Session State

Seen strongest in:

- LangGraph
- OpenFang

Practical meaning:

- do not let transcript be the only continuity mechanism
- persist one compact, authoritative recent-context object

For `teamD`:

- `SessionHead` should become canonical
- it should be visible in API/CLI/TUI/Telegram
- it should update on meaningful run completion
- it should drive continue/recent-work logic

### Pattern 2: File-Based Durable Project Memory

Seen strongest in:

- Claude Code

Practical meaning:

- project truth belongs in files
- not in chat history

For `teamD`:

- keep the project knowledge layout skill
- use it for durable operational memory
- treat project files as durable truth, but not as a substitute for session working state

### Pattern 3: Context Class Separation

Seen strongest in:

- OpenHands
- Claude Code

Practical meaning:

- not all context should load the same way

For `teamD`:

Split prompt layers into:

- always-loaded
- trigger-loaded
- on-demand

### Pattern 4: Context Transparency

Seen strongest in:

- OpenClaw

Practical meaning:

- operators should see where the prompt budget goes

For `teamD`:

- add budget breakdown and projected final prompt accounting

### Pattern 5: Pruning Separate From Compaction

Seen strongest in:

- OpenClaw

Practical meaning:

- not all prompt reduction should rewrite durable state

For `teamD`:

- keep compaction for durable checkpoint synthesis
- add pruning for in-memory prompt assembly decisions

### Pattern 6: Runtime-Level Budget Governance

Seen strongest in:

- OpenFang

Practical meaning:

- budget is a runtime concern, not just a UI estimate

For `teamD`:

- move from rough adapter-local percentage display toward runtime-level prompt accounting

## Direct Diagnosis Of teamD Through This Lens

### What teamD Already Has Right

- artifact offload reduces prompt pollution
- checkpoint and continuity already separate working state from transcript
- replay already exists
- SessionHead has started the correct canonical recent-context path
- project knowledge layout is now explicit

This is a strong base.

### What Is Still Missing

#### 1. SessionHead is not yet the full canonical session layer

It exists, but it still needs to become:

- more visible
- more authoritative
- more integrated with command semantics

#### 2. Prompt budget accounting is too opaque

Today the operator sees something like context percentage, but that is not enough to answer:

- what part is transcript
- what part is SessionHead
- what part is recall
- what part is skills
- what part is checkpoint

#### 3. Compaction trigger is based on raw transcript estimates

That is too coarse.

It should eventually consider projected final prompt size, including injected runtime layers.

#### 4. Prefix retention is still too naive

Current fitting logic is mainly:

- protect active tail
- include older prefix while budget allows

That is not semantically strong enough for long sessions.

#### 5. Context classes are still under-specified

`teamD` still risks loading too much "helpful" system material too eagerly.

## Recommended teamD Architecture Direction

This is the distilled recommendation after comparing all systems above.

### Layer 1: Raw Transcript

Purpose:

- audit
- replay
- durable history

Not the canonical recent-context layer.

### Layer 2: SessionHead

Purpose:

- canonical recent-context between runs

Contents should stay compact:

- current goal
- last meaningful result
- recent artifact refs
- unresolved loops
- current project binding
- maybe current plan handle

This should be the first runtime-owned continuity object consulted before recall.

### Layer 3: Checkpoint / Continuity

Purpose:

- compaction product
- broader session working state

This remains useful, but should not compete with `SessionHead` for the role of recent truth.

### Layer 4: Project Files

Purpose:

- durable operational knowledge

Examples:

- procedures
- runbooks
- architecture
- decisions
- state/current

### Layer 5: Searchable Memory

Purpose:

- long-range recall
- retrieval of durable facts

This should not be the first place the system looks for something that happened one run ago.

### Layer 6: Prompt Budget Governance

Purpose:

- decide what earns prompt residency

This should explicitly account for:

- always-loaded
- trigger-loaded
- on-demand
- checkpoint
- SessionHead
- recall
- tail
- old-prefix selection

## Recommended Backlog Direction

The right next architectural line for `teamD` is not "more memory". It is "better state and better budget governance."

### P1

#### 1. SessionHead maturation

- make SessionHead canonical across more control surfaces
- use it for continue/recent-work semantics
- ensure it updates deterministically after meaningful runs

#### 2. Prompt budget transparency

- expose budget breakdown to operator surfaces
- show both full-window percent and prompt-budget percent
- show system overhead explicitly

#### 3. Projected final prompt accounting

- stop triggering compaction based only on raw transcript
- estimate with SessionHead, recall, skills, and workspace included

### P2

#### 4. Context class separation

- always-loaded
- trigger-loaded
- on-demand

#### 5. Pruning layer

- prune old prompt baggage without rewriting durable transcript

#### 6. Old-prefix selection/extraction

- selection first
- extraction second

### P3

#### 7. Stable prompt layers and cache-aware assembly

This is where ideas closer to OpenFang become interesting:

- stable prompt slices
- volatile recent-state slices
- better caching characteristics

But only after the runtime is already clear and observable.

## What Not To Do

- Do not respond to context loss by simply increasing the context window.
- Do not add more recall kinds before making budget accounting visible.
- Do not rely on memory search for just-completed work.
- Do not push mesh into the hot path before delegation and recent-context layers are fully coherent.
- Do not let project files become a substitute for session working state.

## Final Recommendation

The most important conceptual shift for `teamD` is this:

`teamD` should stop treating context as one big fuzzy bucket and start treating it as a stack of explicit layers with different responsibilities.

The borrowed patterns that matter most are:

- from LangGraph:
  - canonical persisted recent state
- from Claude Code:
  - durable project memory in files
- from OpenHands:
  - context class separation
- from OpenClaw:
  - context transparency and pruning
- from OpenFang:
  - canonical session and runtime-level budget governance

If implemented well, the result is not a more complicated system. It is a more legible system:

- recent work stops looking forgotten
- context loss becomes explainable
- operators can see where budget goes
- memory becomes layered instead of vague
- future Telegram UX, TUI, and mesh work all gain a stronger base

## Immediate Practical Translation For teamD

If this document is reduced to a single implementation sequence, it should be this:

1. finish SessionHead maturation
2. add prompt budget transparency
3. move compaction triggering to projected final prompt accounting
4. classify prompt context into always-loaded / trigger-loaded / on-demand
5. add pruning
6. only then improve selection/extraction for older context

That sequence is the shortest path from today's pain to a genuinely better runtime.
