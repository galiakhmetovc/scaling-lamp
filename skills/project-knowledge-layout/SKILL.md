---
name: project-knowledge-layout
description: Use when the user wants a durable file structure for projects, asks how to store project knowledge so the agent can navigate it, or needs canonical README/docs/state/notes/artifacts files.
version: 1
---

# Project Knowledge Layout

Use this skill to keep project knowledge in predictable files so the agent can orient itself without guessing where truth lives.

## Core Rule

Separate:
- project knowledge
- operational state
- raw notes
- artifacts

Do not treat chat memory as the source of truth. Important project context belongs in files.

## Global Registry

When the user manages multiple projects under one root, recommend:

```text
projects/
  index.md
  <project-a>/
  <project-b>/
```

`projects/index.md` should list:
- project name
- path
- purpose
- status
- canonical state file

## Canonical Layout

For each project, prefer:

```text
<project>/
  README.md
  docs/
    architecture.md
    decisions.md
  state/
    current.md
    backlog.md
  notes/
    YYYY-MM-DD.md
  artifacts/
```

If the project already has a strong existing structure, adapt instead of forcing a rewrite. Still add canonical files where they are missing.

## File Roles

- `README.md`
  - what the project is
  - goal
  - current status
  - where the canonical files are
- `docs/architecture.md`
  - structure
  - components
  - data flow
  - key commands and environments
- `docs/decisions.md`
  - durable architectural or product decisions
  - why they were made
- `state/current.md`
  - active work
  - blockers
  - next steps
- `state/backlog.md`
  - upcoming work not yet in motion
- `notes/YYYY-MM-DD.md`
  - raw daily notes and temporary research
- `artifacts/`
  - logs
  - exports
  - traces
  - snapshots

## Reading Order For Agents

When entering a project, read in this order:

1. `README.md`
2. `state/current.md`
3. `docs/architecture.md`
4. `docs/decisions.md`
5. only then other files as needed

## Quick Reference

- New project from scratch:
  - scaffold canonical layout
  - fill `README.md`
  - fill `state/current.md`
  - fill `docs/architecture.md`
- Existing messy project:
  - identify current source-of-truth files
  - add canonical files
  - summarize or link old knowledge into them
  - avoid large moves unless the user asked for cleanup
- Agent entering a project:
  - read `README.md`
  - read `state/current.md`
  - read `docs/architecture.md`
  - read `docs/decisions.md`

## Anti-Patterns

Do not:
- store durable project context only in chat memory
- scatter current state across multiple competing files
- mix raw notes and canonical state in one file
- dump artifacts into `docs/`
- rewrite an established structure without need

## References

- Template: `references/project-template.md`
- Scaffold: `scripts/scaffold-project-layout.sh`
