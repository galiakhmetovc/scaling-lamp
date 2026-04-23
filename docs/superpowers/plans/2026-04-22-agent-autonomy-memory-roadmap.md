# Agent Autonomy and Memory Roadmap

## Goal

Turn the research slice into a concrete implementation order for post-`1.0.0` runtime work.

## Sequence

1. Finish the research docs and gap matrix.
2. Expose canonical schedule-management tools to agents.
3. Add self-addressed one-shot deferred continuation.
4. Add a controlled agent factory path.
5. Define retention and archival policy for historical sessions.
6. Index and search sessions, transcripts, and artifacts.
7. Design operator-approved rollout and self-update workflows.

## Rationale

- Scheduling comes first because it unlocks the most immediately useful autonomy case: “continue later”.
- Self-resume comes before agent factory because it gives more value with less coordination complexity.
- Memory policy comes before memory search so the search surface has stable storage semantics.
- Deployment comes last because it touches safety, audit, and target control.

## Mapping To Beads

- `teamD-research.3`
- `teamD-research.2`
- `teamD-research.1`
- `teamD-autonomy.1`
- `teamD-autonomy.3`
- `teamD-autonomy.2`
- `teamD-memory.1`
- `teamD-memory.2`
- `teamD-deploy.1`

## Deliverables

- one canonical design for agent-facing schedule tools
- one canonical design for one-shot deferred continuation
- one canonical design for agent creation
- one canonical retention and search model for sessions
- one canonical operator-approved rollout workflow
